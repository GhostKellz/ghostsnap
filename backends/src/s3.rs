use crate::backend::{Backend, BackendType, ObjectInfo};
use crate::retry::{RetryConfig, retry_with_backoff};
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;
use ghostsnap_core::{Error, Result};

pub struct S3Backend {
    client: Client,
    bucket: String,
    prefix: String,
    retry_config: RetryConfig,
}

impl S3Backend {
    pub async fn new(bucket: String, prefix: String) -> Result<Self> {
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let client = Client::new(&config);
        
        Ok(Self {
            client,
            bucket,
            prefix,
            retry_config: RetryConfig::default(),
        })
    }
    
    pub async fn with_endpoint(bucket: String, prefix: String, endpoint: String) -> Result<Self> {
        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .load()
            .await;
        let client = Client::new(&config);
        
        Ok(Self {
            client,
            bucket,
            prefix,
            retry_config: RetryConfig::default(),
        })
    }

    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }
    
    fn full_key(&self, path: &str) -> String {
        if self.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.prefix, path)
        }
    }
}

#[async_trait]
impl Backend for S3Backend {
    async fn init(&self) -> Result<()> {
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| Error::Backend(format!("Bucket {} not accessible: {}", self.bucket, e)))?;
        Ok(())
    }
    
    async fn exists(&self, path: &str) -> Result<bool> {
        let result = self.client
            .head_object()
            .bucket(&self.bucket)
            .key(self.full_key(path))
            .send()
            .await;
        
        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.to_string().contains("NotFound") {
                    Ok(false)
                } else {
                    Err(Error::Backend(format!("Failed to check existence: {}", e)))
                }
            }
        }
    }
    
    async fn read(&self, path: &str) -> Result<Bytes> {
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let key = self.full_key(path);
        let path_copy = path.to_string();
        
        retry_with_backoff(&self.retry_config, "s3_read", || async {
            let response = client
                .get_object()
                .bucket(&bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| Error::Backend(format!("Failed to read {}: {}", path_copy, e)))?;
            
            let data = response.body.collect().await
                .map_err(|e| Error::Backend(format!("Failed to read body: {}", e)))?;
            
            Ok(data.into_bytes())
        })
        .await
    }
    
    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let key = self.full_key(path);
        let path_copy = path.to_string();
        
        retry_with_backoff(&self.retry_config, "s3_write", || async {
            let body = ByteStream::from(data.to_vec());
            
            client
                .put_object()
                .bucket(&bucket)
                .key(&key)
                .body(body)
                .send()
                .await
                .map_err(|e| Error::Backend(format!("Failed to write {}: {}", path_copy, e)))?;
            
            Ok(())
        })
        .await
    }
    
    async fn delete(&self, path: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(self.full_key(path))
            .send()
            .await
            .map_err(|e| Error::Backend(format!("Failed to delete {}: {}", path, e)))?;
        
        Ok(())
    }
    
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let full_prefix = self.full_key(prefix);
        let mut results = Vec::new();
        let mut continuation_token = None;
        
        loop {
            let mut request = self.client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&full_prefix);
            
            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }
            
            let response = request.send().await
                .map_err(|e| Error::Backend(format!("Failed to list: {}", e)))?;
            
            if let Some(contents) = response.contents {
                for object in contents {
                    if let Some(key) = object.key {
                        let path = if self.prefix.is_empty() {
                            key
                        } else {
                            key.strip_prefix(&format!("{}/", self.prefix))
                                .unwrap_or(&key)
                                .to_string()
                        };
                        results.push(path);
                    }
                }
            }
            
            if response.is_truncated.unwrap_or(false) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }
        
        Ok(results)
    }
    
    async fn stat(&self, path: &str) -> Result<ObjectInfo> {
        let response = self.client
            .head_object()
            .bucket(&self.bucket)
            .key(self.full_key(path))
            .send()
            .await
            .map_err(|e| Error::Backend(format!("Failed to stat {}: {}", path, e)))?;
        
        let size = response.content_length.unwrap_or(0) as u64;
        let modified = response.last_modified
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
        BackendType::S3
    }
}