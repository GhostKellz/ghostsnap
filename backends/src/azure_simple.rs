use crate::backend::{Backend, BackendType, ObjectInfo};
use crate::retry::{RetryConfig, retry_with_backoff};
use async_trait::async_trait;
use azure_identity::DeveloperToolsCredential;
use azure_storage_blob::clients::BlobContainerClient;
use azure_storage_blob::models::{
    BlobClientGetPropertiesResultHeaders, BlobContainerClientListBlobsOptions,
};
use bytes::Bytes;
use ghostsnap_core::{Error, Result};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AzureConfig {
    pub account_name: String,
    pub container: String,
    pub prefix: String,
    /// Account key (if not using managed identity)
    pub account_key: Option<String>,
}

pub struct AzureBackend {
    client: BlobContainerClient,
    prefix: String,
    retry_config: RetryConfig,
}

impl AzureBackend {
    /// Create a new Azure backend.
    ///
    /// Authentication resolves in this order:
    /// 1. SAS token from `AZURE_STORAGE_SAS_TOKEN` (or `AZURE_STORAGE_SAS`).
    /// 2. Microsoft Entra ID via the standard credential chain.
    ///
    /// A custom endpoint may be supplied via `AZURE_STORAGE_ENDPOINT`.
    pub async fn new(account_name: String, container: String) -> Result<Self> {
        let client = Self::build_container_client(&account_name, &container)?;

        Ok(Self {
            client,
            prefix: String::new(),
            retry_config: RetryConfig::default(),
        })
    }

    fn build_container_client(
        account_name: &str,
        container: &str,
    ) -> Result<BlobContainerClient> {
        let endpoint = std::env::var("AZURE_STORAGE_ENDPOINT")
            .unwrap_or_else(|_| format!("https://{}.blob.core.windows.net", account_name));
        let endpoint = endpoint.trim_end_matches('/');

        if let Ok(sas) = std::env::var("AZURE_STORAGE_SAS_TOKEN")
            .or_else(|_| std::env::var("AZURE_STORAGE_SAS"))
        {
            let sas = sas.trim_start_matches('?');
            let url = Url::parse(&format!("{}/{}?{}", endpoint, container, sas))
                .map_err(|e| Error::Backend(format!("Invalid Azure URL: {}", e)))?;
            return BlobContainerClient::new(url, None, None)
                .map_err(|e| Error::Backend(format!("Failed to create Azure client: {}", e)));
        }

        let credential = DeveloperToolsCredential::new(None).map_err(|e| {
            Error::Backend(format!(
                "Failed to create Azure credential (set AZURE_STORAGE_SAS_TOKEN for SAS auth, \
                 or configure Microsoft Entra ID): {}",
                e
            ))
        })?;
        let url = Url::parse(&format!("{}/{}", endpoint, container))
            .map_err(|e| Error::Backend(format!("Invalid Azure URL: {}", e)))?;
        BlobContainerClient::new(url, Some(credential), None)
            .map_err(|e| Error::Backend(format!("Failed to create Azure client: {}", e)))
    }

    pub fn with_prefix(mut self, prefix: String) -> Self {
        self.prefix = prefix;
        self
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
impl Backend for AzureBackend {
    async fn init(&self) -> Result<()> {
        // Check if container exists, create if not
        match self.client.exists().await {
            Ok(true) => Ok(()),
            Ok(false) => {
                self.client
                    .create(None)
                    .await
                    .map_err(|e| Error::Backend(format!("Failed to create container: {}", e)))?;
                Ok(())
            }
            Err(e) => Err(Error::Backend(format!(
                "Failed to check container existence: {}",
                e
            ))),
        }
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let blob_client = self.client.blob_client(&self.full_key(path));
        match blob_client.exists().await {
            Ok(exists) => Ok(exists),
            Err(e) => Err(Error::Backend(format!("Failed to check existence: {}", e))),
        }
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        let blob_client = self.client.blob_client(&self.full_key(path));
        let path_copy = path.to_string();

        retry_with_backoff(&self.retry_config, "azure_read", || async {
            let response = blob_client
                .download(None)
                .await
                .map_err(|e| Error::Backend(format!("Failed to read {}: {}", path_copy, e)))?;

            response
                .body
                .collect()
                .await
                .map_err(|e| Error::Backend(format!("Failed to read body {}: {}", path_copy, e)))
        })
        .await
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let blob_client = self.client.blob_client(&self.full_key(path));
        let path_copy = path.to_string();

        retry_with_backoff(&self.retry_config, "azure_write", || async {
            blob_client
                .upload(data.clone().into(), None)
                .await
                .map_err(|e| Error::Backend(format!("Failed to write {}: {}", path_copy, e)))?;

            Ok(())
        })
        .await
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let blob_client = self.client.blob_client(&self.full_key(path));

        blob_client
            .delete(None)
            .await
            .map_err(|e| Error::Backend(format!("Failed to delete {}: {}", path, e)))?;

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let full_prefix = self.full_key(prefix);
        let mut results = Vec::new();

        let options = BlobContainerClientListBlobsOptions {
            prefix: Some(full_prefix.clone()),
            ..Default::default()
        };

        let mut pager = self
            .client
            .list_blobs(Some(options))
            .map_err(|e| Error::Backend(format!("Failed to list blobs: {}", e)))?;

        // The pager flattens pages into individual blob items.
        use futures::StreamExt;
        while let Some(blob) = pager.next().await {
            let blob =
                blob.map_err(|e| Error::Backend(format!("Failed to list blobs: {}", e)))?;

            let Some(blob_name) = blob.name else {
                continue;
            };

            let path = if self.prefix.is_empty() {
                blob_name.clone()
            } else {
                blob_name
                    .strip_prefix(&format!("{}/", self.prefix))
                    .unwrap_or(&blob_name)
                    .to_string()
            };
            results.push(path);
        }

        Ok(results)
    }

    async fn stat(&self, path: &str) -> Result<ObjectInfo> {
        let blob_client = self.client.blob_client(&self.full_key(path));

        let response = blob_client
            .get_properties(None)
            .await
            .map_err(|e| Error::Backend(format!("Failed to stat {}: {}", path, e)))?;

        let size = response.content_length().unwrap_or(None).unwrap_or(0);
        let modified = response
            .last_modified()
            .ok()
            .flatten()
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.unix_timestamp(), 0))
            .unwrap_or_else(chrono::Utc::now);

        Ok(ObjectInfo {
            path: path.to_string(),
            size,
            modified,
        })
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Azure
    }
}

// Keep the simple placeholder struct for backwards compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureSimpleBackend {
    pub account_name: String,
    pub container: String,
    pub prefix: String,
}

impl AzureSimpleBackend {
    pub fn new(account_name: String, container: String) -> Self {
        Self {
            account_name,
            container,
            prefix: String::new(),
        }
    }
}
