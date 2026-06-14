//! Backblaze B2 native API backend.
//!
//! This backend uses the native B2 API for better performance and features
//! compared to S3-compatible mode.

use crate::backend::{Backend, BackendType, ObjectInfo};
use crate::retry::{RetryConfig, retry_with_backoff};
use async_trait::async_trait;
use bytes::Bytes;
use ghostsnap_core::{Error, Result};
use reqwest::{Client, header};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// B2 authorization response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct AuthResponse {
    account_id: String,
    authorization_token: String,
    api_url: String,
    download_url: String,
    #[serde(default)]
    recommended_part_size: u64,
}

/// B2 upload URL response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct UploadUrlResponse {
    bucket_id: String,
    upload_url: String,
    authorization_token: String,
}

/// B2 file info response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileInfo {
    file_id: String,
    file_name: String,
    content_length: u64,
    upload_timestamp: u64,
}

/// B2 list files response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFilesResponse {
    files: Vec<FileInfo>,
    next_file_name: Option<String>,
}

/// B2 backend configuration
#[derive(Debug, Clone)]
pub struct B2Config {
    pub application_key_id: String,
    pub application_key: String,
    pub bucket_name: String,
    pub bucket_id: String,
    pub prefix: String,
}

/// Authorization state (cached)
#[allow(dead_code)]
struct AuthState {
    auth: AuthResponse,
    upload_url: Option<UploadUrlResponse>,
    expires_at: std::time::Instant,
}

pub struct B2Backend {
    config: B2Config,
    client: Client,
    auth_state: Arc<RwLock<Option<AuthState>>>,
    retry_config: RetryConfig,
}

impl B2Backend {
    pub fn new(config: B2Config) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| Error::Backend(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            config,
            client,
            auth_state: Arc::new(RwLock::new(None)),
            retry_config: RetryConfig::default(),
        })
    }

    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }

    async fn ensure_auth(&self) -> Result<AuthResponse> {
        // Check if we have valid cached auth
        {
            let state = self.auth_state.read().await;
            if let Some(ref s) = *state
                && s.expires_at > std::time::Instant::now()
            {
                return Ok(s.auth.clone());
            }
        }

        // Need to re-authorize
        let auth = self.authorize().await?;

        // Cache the auth
        {
            let mut state = self.auth_state.write().await;
            *state = Some(AuthState {
                auth: auth.clone(),
                upload_url: None,
                expires_at: std::time::Instant::now() + std::time::Duration::from_secs(3600),
            });
        }

        Ok(auth)
    }

    async fn authorize(&self) -> Result<AuthResponse> {
        use base64::Engine;
        let auth_string = format!(
            "{}:{}",
            self.config.application_key_id, self.config.application_key
        );
        let auth_header = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(&auth_string)
        );

        let response = self
            .client
            .get("https://api.backblazeb2.com/b2api/v2/b2_authorize_account")
            .header(header::AUTHORIZATION, auth_header)
            .send()
            .await
            .map_err(|e| Error::Backend(format!("B2 auth failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Backend(format!(
                "B2 auth failed ({}): {}",
                status, body
            )));
        }

        response
            .json::<AuthResponse>()
            .await
            .map_err(|e| Error::Backend(format!("B2 auth parse failed: {}", e)))
    }

    async fn get_upload_url(&self) -> Result<UploadUrlResponse> {
        let auth = self.ensure_auth().await?;

        let url = format!("{}/b2api/v2/b2_get_upload_url", auth.api_url);

        let body = serde_json::json!({
            "bucketId": self.config.bucket_id
        });

        let response = self
            .client
            .post(&url)
            .header(header::AUTHORIZATION, &auth.authorization_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Backend(format!("B2 get_upload_url failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Backend(format!(
                "B2 get_upload_url failed ({}): {}",
                status, body
            )));
        }

        response
            .json::<UploadUrlResponse>()
            .await
            .map_err(|e| Error::Backend(format!("B2 upload_url parse failed: {}", e)))
    }

    fn full_path(&self, path: &str) -> String {
        if self.config.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.config.prefix, path)
        }
    }
}

#[async_trait]
impl Backend for B2Backend {
    async fn init(&self) -> Result<()> {
        // Just verify we can authenticate
        self.ensure_auth().await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let auth = self.ensure_auth().await?;
        let full_path = self.full_path(path);

        let url = format!("{}/b2api/v2/b2_list_file_names", auth.api_url);

        let body = serde_json::json!({
            "bucketId": self.config.bucket_id,
            "prefix": full_path,
            "maxFileCount": 1
        });

        let response = self
            .client
            .post(&url)
            .header(header::AUTHORIZATION, &auth.authorization_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Backend(format!("B2 list failed: {}", e)))?;

        if !response.status().is_success() {
            return Ok(false);
        }

        let list: ListFilesResponse = response
            .json()
            .await
            .map_err(|e| Error::Backend(format!("B2 list parse failed: {}", e)))?;

        Ok(list.files.iter().any(|f| f.file_name == full_path))
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        let auth = self.ensure_auth().await?;
        let full_path = self.full_path(path);

        let url = format!(
            "{}/file/{}/{}",
            auth.download_url, self.config.bucket_name, full_path
        );

        let client = self.client.clone();
        let auth_token = auth.authorization_token.clone();

        retry_with_backoff(&self.retry_config, "b2_read", || async {
            let response = client
                .get(&url)
                .header(header::AUTHORIZATION, &auth_token)
                .send()
                .await
                .map_err(|e| Error::Backend(format!("B2 read failed: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(Error::Backend(format!(
                    "B2 read failed ({}): {}",
                    status, body
                )));
            }

            let bytes = response
                .bytes()
                .await
                .map_err(|e| Error::Backend(format!("B2 read body failed: {}", e)))?;

            Ok(bytes)
        })
        .await
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let upload_url = self.get_upload_url().await?;
        let full_path = self.full_path(path);

        // Calculate SHA1 hash
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(&data);
        let sha1_hash = hex::encode(hasher.finalize());

        let client = self.client.clone();

        retry_with_backoff(&self.retry_config, "b2_write", || async {
            let response = client
                .post(&upload_url.upload_url)
                .header(header::AUTHORIZATION, &upload_url.authorization_token)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::CONTENT_LENGTH, data.len())
                .header("X-Bz-File-Name", urlencoding::encode(&full_path).as_ref())
                .header("X-Bz-Content-Sha1", &sha1_hash)
                .body(data.to_vec())
                .send()
                .await
                .map_err(|e| Error::Backend(format!("B2 write failed: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(Error::Backend(format!(
                    "B2 write failed ({}): {}",
                    status, body
                )));
            }

            Ok(())
        })
        .await
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let auth = self.ensure_auth().await?;
        let full_path = self.full_path(path);

        // First, get the file ID
        let list_url = format!("{}/b2api/v2/b2_list_file_names", auth.api_url);
        let list_body = serde_json::json!({
            "bucketId": self.config.bucket_id,
            "prefix": full_path,
            "maxFileCount": 1
        });

        let list_response = self
            .client
            .post(&list_url)
            .header(header::AUTHORIZATION, &auth.authorization_token)
            .json(&list_body)
            .send()
            .await
            .map_err(|e| Error::Backend(format!("B2 list for delete failed: {}", e)))?;

        let list: ListFilesResponse = list_response
            .json()
            .await
            .map_err(|e| Error::Backend(format!("B2 list parse failed: {}", e)))?;

        let file = list
            .files
            .iter()
            .find(|f| f.file_name == full_path)
            .ok_or_else(|| Error::Backend(format!("File not found: {}", path)))?;

        // Now delete by file ID
        let delete_url = format!("{}/b2api/v2/b2_delete_file_version", auth.api_url);
        let delete_body = serde_json::json!({
            "fileId": file.file_id,
            "fileName": full_path
        });

        let response = self
            .client
            .post(&delete_url)
            .header(header::AUTHORIZATION, &auth.authorization_token)
            .json(&delete_body)
            .send()
            .await
            .map_err(|e| Error::Backend(format!("B2 delete failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Backend(format!(
                "B2 delete failed ({}): {}",
                status, body
            )));
        }

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let auth = self.ensure_auth().await?;
        let full_prefix = self.full_path(prefix);

        let mut results = Vec::new();
        let mut next_file_name: Option<String> = None;

        loop {
            let url = format!("{}/b2api/v2/b2_list_file_names", auth.api_url);

            let mut body = serde_json::json!({
                "bucketId": self.config.bucket_id,
                "prefix": full_prefix,
                "maxFileCount": 1000
            });

            if let Some(ref next) = next_file_name {
                body["startFileName"] = serde_json::Value::String(next.clone());
            }

            let response = self
                .client
                .post(&url)
                .header(header::AUTHORIZATION, &auth.authorization_token)
                .json(&body)
                .send()
                .await
                .map_err(|e| Error::Backend(format!("B2 list failed: {}", e)))?;

            if !response.status().is_success() {
                break;
            }

            let list: ListFilesResponse = response
                .json()
                .await
                .map_err(|e| Error::Backend(format!("B2 list parse failed: {}", e)))?;

            for file in list.files {
                let path = if self.config.prefix.is_empty() {
                    file.file_name
                } else {
                    file.file_name
                        .strip_prefix(&format!("{}/", self.config.prefix))
                        .unwrap_or(&file.file_name)
                        .to_string()
                };
                results.push(path);
            }

            next_file_name = list.next_file_name;
            if next_file_name.is_none() {
                break;
            }
        }

        Ok(results)
    }

    async fn stat(&self, path: &str) -> Result<ObjectInfo> {
        let auth = self.ensure_auth().await?;
        let full_path = self.full_path(path);

        let url = format!("{}/b2api/v2/b2_list_file_names", auth.api_url);
        let body = serde_json::json!({
            "bucketId": self.config.bucket_id,
            "prefix": full_path,
            "maxFileCount": 1
        });

        let response = self
            .client
            .post(&url)
            .header(header::AUTHORIZATION, &auth.authorization_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Backend(format!("B2 stat failed: {}", e)))?;

        let list: ListFilesResponse = response
            .json()
            .await
            .map_err(|e| Error::Backend(format!("B2 stat parse failed: {}", e)))?;

        let file = list
            .files
            .iter()
            .find(|f| f.file_name == full_path)
            .ok_or_else(|| Error::Backend(format!("File not found: {}", path)))?;

        let modified = chrono::DateTime::from_timestamp_millis(file.upload_timestamp as i64)
            .unwrap_or_else(chrono::Utc::now);

        Ok(ObjectInfo {
            path: path.to_string(),
            size: file.content_length,
            modified,
        })
    }

    fn backend_type(&self) -> BackendType {
        BackendType::B2
    }
}
