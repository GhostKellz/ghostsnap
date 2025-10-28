use crate::backend::{Backend, BackendType, ObjectInfo};
use crate::retry::{retry_with_backoff, RetryConfig};
use async_trait::async_trait;
use aws_config::Region;
use aws_sdk_s3::{
    Client, 
    config::{Credentials, Builder as S3ConfigBuilder},
    operation::put_object::PutObjectOutput,
    types::{CompletedMultipartUpload, CompletedPart, StorageClass, ServerSideEncryption},
    primitives::ByteStream,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bytes::Bytes;
use ghostsnap_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinIOConfig {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    pub prefix: String,
    pub region: String,
    pub use_ssl: bool,
    pub path_style: bool,
    pub multipart_threshold: usize,
    pub chunk_size: usize,
    pub max_concurrency: usize,
    pub storage_class: Option<String>,
    pub server_side_encryption: Option<String>,
    pub retry_attempts: u32,
    pub retry_delay_ms: u64,
    pub bandwidth_limit_mbps: Option<f64>,
    pub enable_checksums: bool,
    pub enable_versioning: bool,
}

impl Default for MinIOConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9000".to_string(),
            access_key: "".to_string(),
            secret_key: "".to_string(),
            bucket: "ghostsnap-backup".to_string(),
            prefix: "".to_string(),
            region: "us-east-1".to_string(),
            use_ssl: false,
            path_style: true, // MinIO typically uses path-style
            multipart_threshold: 64 * 1024 * 1024, // 64MB
            chunk_size: 16 * 1024 * 1024, // 16MB per part
            max_concurrency: 8,
            storage_class: None,
            server_side_encryption: None,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            bandwidth_limit_mbps: None,
            enable_checksums: true,
            enable_versioning: false,
        }
    }
}

pub struct MinIOBackend {
    client: Client,
    config: MinIOConfig,
    #[allow(dead_code)] // Future feature: bandwidth limiting
    bandwidth_limiter: Option<BandwidthLimiter>,
    retry_config: RetryConfig,
}

#[allow(dead_code)] // Future feature: bandwidth limiting
struct BandwidthLimiter {
    max_bytes_per_second: f64,
    last_check: std::time::Instant,
    bytes_used: usize,
}

#[allow(dead_code)] // Future feature: bandwidth limiting
impl BandwidthLimiter {
    fn new(mbps: f64) -> Self {
        Self {
            max_bytes_per_second: mbps * 1024.0 * 1024.0,
            last_check: std::time::Instant::now(),
            bytes_used: 0,
        }
    }
    
    async fn throttle(&mut self, bytes: usize) {
        self.bytes_used += bytes;
        
        let elapsed = self.last_check.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            // Reset counters every second
            self.last_check = std::time::Instant::now();
            self.bytes_used = 0;
            return;
        }
        
        let bytes_per_second = self.bytes_used as f64 / elapsed;
        if bytes_per_second > self.max_bytes_per_second {
            let required_delay = (self.bytes_used as f64 / self.max_bytes_per_second) - elapsed;
            if required_delay > 0.0 {
                sleep(Duration::from_secs_f64(required_delay)).await;
            }
        }
    }
}

impl MinIOBackend {
    pub async fn new(config: MinIOConfig) -> Result<Self> {
        let credentials = Credentials::new(
            &config.access_key,
            &config.secret_key,
            None,
            None,
            "ghostsnap-minio",
        );
        
        let s3_config = S3ConfigBuilder::new()
            .credentials_provider(credentials)
            .region(Region::new(config.region.clone()))
            .endpoint_url(&config.endpoint)
            .force_path_style(config.path_style)
            .build();
        
        let client = Client::from_conf(s3_config);
        
        let bandwidth_limiter = config.bandwidth_limit_mbps
            .map(BandwidthLimiter::new);
        
        let backend = Self { 
            client, 
            config: config.clone(),
            bandwidth_limiter: bandwidth_limiter.into(),
            retry_config: RetryConfig::default(), // Use default retry config
        };
        
        backend.ensure_bucket_exists().await?;
        Ok(backend)
    }
    
    /// Configure custom retry behavior
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }
    
    pub async fn with_credentials(
        endpoint: String,
        access_key: String,
        secret_key: String,
        bucket: String,
    ) -> Result<Self> {
        let config = MinIOConfig {
            endpoint,
            access_key,
            secret_key,
            bucket,
            ..Default::default()
        };
        Self::new(config).await
    }
    
    async fn ensure_bucket_exists(&self) -> Result<()> {
        let bucket = self.config.bucket.clone();
        let client = self.client.clone();
        
        match client
            .head_bucket()
            .bucket(&bucket)
            .send()
            .await 
        {
            Ok(_) => Ok(()),
            Err(_) => {
                // Bucket doesn't exist, try to create it
                retry_with_backoff(&self.retry_config, "minio_create_bucket", || async {
                    client
                        .create_bucket()
                        .bucket(&bucket)
                        .send()
                        .await
                        .map_err(|e| Error::Backend(format!("Failed to create bucket: {:?}", e)))
                }).await?;
                
                // Configure versioning if enabled
                if self.config.enable_versioning {
                    self.enable_versioning().await?;
                }
                
                Ok(())
            }
        }
    }
    
    async fn enable_versioning(&self) -> Result<()> {
        use aws_sdk_s3::types::{VersioningConfiguration, BucketVersioningStatus};
        
        let versioning_config = VersioningConfiguration::builder()
            .status(BucketVersioningStatus::Enabled)
            .build();
        
        let bucket = self.config.bucket.clone();
        let client = self.client.clone();
        
        retry_with_backoff(&self.retry_config, "minio_enable_versioning", || async {
            client
                .put_bucket_versioning()
                .bucket(&bucket)
                .versioning_configuration(versioning_config.clone())
                .send()
                .await
                .map_err(|e| Error::Backend(format!("Failed to enable versioning: {:?}", e)))
        }).await?;
        
        Ok(())
    }
    
    fn full_key(&self, path: &str) -> String {
        if self.config.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.config.prefix, path)
        }
    }
    
    // Note: Bandwidth throttling not yet implemented
    // Will be enabled in future version with interior mutability pattern
    #[allow(dead_code)]
    async fn throttle_if_needed(&self, _bytes: usize) {
        // TODO: Implement with Mutex<BandwidthLimiter> for interior mutability
    }
    
    #[allow(dead_code)] // Used when multipart threshold is set very high
    async fn simple_upload(&self, path: &str, data: Bytes) -> Result<PutObjectOutput> {
        let data_len = data.len();
        self.throttle_if_needed(data_len).await;
        
        let data_clone = data.clone();
        let bucket = self.config.bucket.clone();
        let key = self.full_key(path);
        let storage_class = self.config.storage_class.clone();
        let server_side_encryption = self.config.server_side_encryption.clone();
        let enable_checksums = self.config.enable_checksums;
        let client = self.client.clone();
        
        retry_with_backoff(&self.retry_config, "minio_simple_upload", || async {
            let mut req = client
                .put_object()
                .bucket(&bucket)
                .key(&key)
                .body(ByteStream::from(data_clone.clone()));
            
            if let Some(ref storage_class) = storage_class {
                // Parse is infallible for AWS SDK enums, so we can match directly
                let sc = storage_class.parse::<StorageClass>().unwrap();
                req = req.storage_class(sc);
            }
            
            if let Some(ref sse) = server_side_encryption {
                // Parse is infallible for AWS SDK enums, so we can directly unwrap
                let encryption = sse.parse::<ServerSideEncryption>().unwrap();
                req = req.server_side_encryption(encryption);
            }
            
            if enable_checksums {
                req = req.content_md5(BASE64.encode(md5::compute(&data_clone).as_ref()));
            }
            
            req.send().await
                .map_err(|e| Error::Backend(format!("Failed to upload: {:?}", e)))
        }).await
    }
    
    async fn multipart_upload(&self, path: &str, data: Bytes) -> Result<()> {
        let key = self.full_key(path);
        let bucket = self.config.bucket.clone();
        let client = self.client.clone();
        let storage_class = self.config.storage_class.clone();
        let server_side_encryption = self.config.server_side_encryption.clone();
        
        // Initiate multipart upload
        let create_response = retry_with_backoff(&self.retry_config, "minio_create_multipart", || async {
            let mut request = client
                .create_multipart_upload()
                .bucket(&bucket)
                .key(&key);
            
            if let Some(ref storage_class) = storage_class {
                // Parse is infallible for AWS SDK enums, so we can directly unwrap
                let sc = storage_class.parse::<StorageClass>().unwrap();
                request = request.storage_class(sc);
            }

            if let Some(ref sse) = server_side_encryption {
                // Parse is infallible for AWS SDK enums, so we can directly unwrap
                let encryption = sse.parse::<ServerSideEncryption>().unwrap();
                request = request.server_side_encryption(encryption);
            }
            
            request.send().await
                .map_err(|e| Error::Backend(format!("Failed to create multipart upload: {:?}", e)))
        }).await?;
        
        let upload_id = create_response.upload_id()
            .ok_or_else(|| Error::Backend("No upload ID returned".to_string()))?
            .to_string();
        
        // Upload parts
        let chunks: Vec<_> = data
            .chunks(self.config.chunk_size)
            .enumerate()
            .map(|(i, chunk)| (i + 1, Bytes::copy_from_slice(chunk)))
            .collect();
        
        let mut completed_parts = Vec::new();
        
        for (part_number, chunk_data) in chunks {
            let chunk_len = chunk_data.len();
            self.throttle_if_needed(chunk_len).await;
            
            let upload_id_clone = upload_id.clone();
            let bucket_clone = bucket.clone();
            let key_clone = key.clone();
            let client_clone = client.clone();
            let enable_checksums = self.config.enable_checksums;
            
            let part_response = retry_with_backoff(&self.retry_config, "minio_upload_part", || async {
                let mut request = client_clone
                    .upload_part()
                    .bucket(&bucket_clone)
                    .key(&key_clone)
                    .upload_id(&upload_id_clone)
                    .part_number(part_number as i32)
                    .body(ByteStream::from(chunk_data.clone()));
                
                if enable_checksums {
                    request = request.content_md5(
                        BASE64.encode(md5::compute(&chunk_data).as_ref())
                    );
                }
                
                request.send().await
                    .map_err(|e| Error::Backend(format!("Failed to upload part: {:?}", e)))
            }).await?;
            
            let completed_part = CompletedPart::builder()
                .part_number(part_number as i32)
                .e_tag(part_response.e_tag().unwrap_or_default())
                .build();
            
            completed_parts.push(completed_part);
        }
        
        // Complete multipart upload
        let completed_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();
        
        retry_with_backoff(&self.retry_config, "minio_complete_multipart", || async {
            client
                .complete_multipart_upload()
                .bucket(&bucket)
                .key(&key)
                .upload_id(&upload_id)
                .multipart_upload(completed_upload.clone())
                .send()
                .await
                .map_err(|e| Error::Backend(format!("Failed to complete multipart upload: {:?}", e)))
        }).await?;
        
        Ok(())
    }
    
    pub async fn get_bucket_metrics(&self) -> Result<BucketMetrics> {
        // Get bucket size and object count (if supported by MinIO)
        let mut total_size = 0;
        let mut object_count = 0;
        
        let mut continuation_token = None;
        
        loop {
            let mut request = self.client
                .list_objects_v2()
                .bucket(&self.config.bucket);
            
            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }
            
            let page = request.send().await
                .map_err(|e| Error::Backend(format!("Failed to list objects: {:?}", e)))?;
            
            for object in page.contents() {
                total_size += object.size().unwrap_or(0) as u64;
                object_count += 1;
            }
            
            if page.is_truncated().unwrap_or(false) {
                continuation_token = page.next_continuation_token().map(|t| t.to_string());
            } else {
                break;
            }
        }
        
        Ok(BucketMetrics {
            total_size,
            object_count,
            bucket_name: self.config.bucket.clone(),
        })
    }
    
    pub async fn set_lifecycle_policy(&self, _days_to_archive: i32, _days_to_delete: i32) -> Result<()> {
        // Lifecycle policy implementation would go here
        // Simplified for now due to AWS SDK complexity
        warn!("Lifecycle policy setting not yet implemented");
        Ok(())
    }
}

#[derive(Debug)]
pub struct BucketMetrics {
    pub total_size: u64,
    pub object_count: u64,
    pub bucket_name: String,
}

#[async_trait]
impl Backend for MinIOBackend {
    async fn init(&self) -> Result<()> {
        self.ensure_bucket_exists().await
    }
    
    async fn exists(&self, path: &str) -> Result<bool> {
        let bucket = self.config.bucket.clone();
        let key = self.full_key(path);
        let client = self.client.clone();
        
        match retry_with_backoff(&self.retry_config, "minio_exists", || async {
            client
                .head_object()
                .bucket(&bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| Error::Backend(format!("Failed to check existence: {:?}", e)))
        }).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    async fn read(&self, path: &str) -> Result<Bytes> {
        let bucket = self.config.bucket.clone();
        let key = self.full_key(path);
        let client = self.client.clone();
        
        let response = retry_with_backoff(&self.retry_config, "minio_read", || async {
            client
                .get_object()
                .bucket(&bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| Error::Backend(format!("Failed to read object {}: {:?}", path, e)))
        }).await?;
        
        let data = response.body.collect().await
            .map_err(|e| Error::Backend(format!("Failed to collect object data: {}", e)))?;
        
        Ok(data.into_bytes())
    }
    
    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let data_len = data.len();
        
        // Use multipart upload for large files
        if data_len >= self.config.multipart_threshold {
            tracing::debug!(
                "Using multipart upload for {} bytes (threshold: {} bytes)",
                data_len,
                self.config.multipart_threshold
            );
            return self.multipart_upload(path, data).await;
        }
        
        // Use simple upload for small files
        tracing::debug!("Using simple upload for {} bytes", data_len);
        let bucket = self.config.bucket.clone();
        let key = self.full_key(path);
        let client = self.client.clone();
        
        retry_with_backoff(&self.retry_config, "minio_write", || async {
            client
                .put_object()
                .bucket(&bucket)
                .key(&key)
                .body(ByteStream::from(data.clone()))
                .send()
                .await
                .map_err(|e| Error::Backend(format!("Failed to write object: {:?}", e)))
        }).await?;
        
        Ok(())
    }
    
    async fn delete(&self, path: &str) -> Result<()> {
        let bucket = self.config.bucket.clone();
        let key = self.full_key(path);
        let client = self.client.clone();
        
        retry_with_backoff(&self.retry_config, "minio_delete", || async {
            client
                .delete_object()
                .bucket(&bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| Error::Backend(format!("Failed to delete object {}: {:?}", path, e)))
        }).await?;
        
        Ok(())
    }
    
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let full_prefix = self.full_key(prefix);
        let mut results = Vec::new();
        
        let mut continuation_token = None;
        
        loop {
            let mut request = self.client
                .list_objects_v2()
                .bucket(&self.config.bucket)
                .prefix(full_prefix.clone());
            
            if let Some(token) = continuation_token.clone() {
                request = request.continuation_token(token);
            }
            
            let page = request.send().await
                .map_err(|e| Error::Backend(format!("Failed to list objects: {:?}", e)))?;
            
            for object in page.contents() {
                if let Some(key) = object.key() {
                    let path = if self.config.prefix.is_empty() {
                        key.to_string()
                    } else {
                        key.strip_prefix(&format!("{}/", self.config.prefix))
                            .unwrap_or(key)
                            .to_string()
                    };
                    results.push(path);
                }
            }
            
            if page.is_truncated().unwrap_or(false) {
                continuation_token = page.next_continuation_token().map(|t| t.to_string());
            } else {
                break;
            }
        }
        
        Ok(results)
    }
    
    async fn stat(&self, path: &str) -> Result<ObjectInfo> {
        let bucket = self.config.bucket.clone();
        let key = self.full_key(path);
        let client = self.client.clone();
        
        let response = retry_with_backoff(&self.retry_config, "minio_stat", || async {
            client
                .head_object()
                .bucket(&bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| Error::Backend(format!("Failed to stat object {}: {:?}", path, e)))
        }).await?;
        
        let size = response.content_length().unwrap_or(0) as u64;
        let modified = response.last_modified()
            .map(|t| {
                let secs = t.secs();
                chrono::DateTime::from_timestamp(secs, 0).unwrap_or_else(chrono::Utc::now)
            })
            .unwrap_or_else(chrono::Utc::now);
        
        Ok(ObjectInfo {
            path: path.to_string(),
            size,
            modified,
        })
    }
    
    fn backend_type(&self) -> BackendType {
        BackendType::S3 // MinIO is S3-compatible
    }
}

use base64;
use md5;