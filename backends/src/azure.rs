use crate::backend::{Backend, BackendType, ObjectInfo};
use crate::retry::{retry_with_backoff, RetryConfig};
use async_trait::async_trait;
use azure_core::auth::TokenCredential;
use azure_identity::{DefaultAzureCredential, ClientSecretCredential};
use azure_storage::StorageCredentials;
use azure_storage_blobs::{
    BlobServiceClient, 
    blob::{BlobClient, AccessTier},
    container::operations::BlobItem,
};
use bytes::Bytes;
use ghostsnap_core::{Error, Result};
use std::sync::Arc;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AzureAuthMethod {
    ConnectionString(String),
    SasToken {
        account_name: String,
        sas_token: String,
    },
    ManagedIdentity {
        account_name: String,
        client_id: Option<String>,
    },
    ServicePrincipal {
        account_name: String,
        client_id: String,
        client_secret: String,
        tenant_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct AzureBlobConfig {
    pub auth: AzureAuthMethod,
    pub container: String,
    pub prefix: String,
    pub access_tier: Option<AccessTier>,
    pub max_concurrency: usize,
    pub chunk_size: usize,
    pub enable_soft_delete: bool,
    pub versioning_enabled: bool,
    pub retry_attempts: u32,
    pub retry_delay_ms: u64,
}

impl Default for AzureBlobConfig {
    fn default() -> Self {
        Self {
            auth: AzureAuthMethod::ManagedIdentity {
                account_name: "".to_string(),
                client_id: None,
            },
            container: "ghostsnap-backup".to_string(),
            prefix: "".to_string(),
            access_tier: Some(AccessTier::Hot),
            max_concurrency: 8,
            chunk_size: 4 * 1024 * 1024, // 4MB chunks for multipart
            enable_soft_delete: true,
            versioning_enabled: false,
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}

pub struct AzureBlobBackend {
    client: BlobServiceClient,
    config: AzureBlobConfig,
    retry_config: RetryConfig,
}

impl AzureBlobBackend {
    pub async fn new(config: AzureBlobConfig) -> Result<Self> {
        let credentials = Self::create_credentials(&config.auth).await?;
        let client = BlobServiceClient::new(&Self::extract_account_name(&config.auth), credentials);
        
        let backend = Self { 
            client, 
            config,
            retry_config: RetryConfig::default(),
        };
        backend.ensure_container_exists().await?;
        Ok(backend)
    }
    
    /// Configure custom retry behavior
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }
    
    pub async fn with_connection_string(
        connection_string: String,
        container: String,
    ) -> Result<Self> {
        let config = AzureBlobConfig {
            auth: AzureAuthMethod::ConnectionString(connection_string),
            container,
            ..Default::default()
        };
        Self::new(config).await
    }
    
    pub async fn with_managed_identity(
        account_name: String,
        container: String,
        client_id: Option<String>,
    ) -> Result<Self> {
        let config = AzureBlobConfig {
            auth: AzureAuthMethod::ManagedIdentity {
                account_name,
                client_id,
            },
            container,
            ..Default::default()
        };
        Self::new(config).await
    }
    
    pub async fn with_service_principal(
        account_name: String,
        container: String,
        client_id: String,
        client_secret: String,
        tenant_id: String,
    ) -> Result<Self> {
        let config = AzureBlobConfig {
            auth: AzureAuthMethod::ServicePrincipal {
                account_name,
                client_id,
                client_secret,
                tenant_id,
            },
            container,
            ..Default::default()
        };
        Self::new(config).await
    }
    
    async fn create_credentials(auth: &AzureAuthMethod) -> Result<StorageCredentials> {
        match auth {
            AzureAuthMethod::ConnectionString(conn_str) => {
                Ok(StorageCredentials::connection_string(conn_str)
                    .map_err(|e| Error::Backend(format!("Invalid connection string: {}", e)))?)
            },
            AzureAuthMethod::SasToken { account_name, sas_token } => {
                Ok(StorageCredentials::sas_token(account_name.clone(), sas_token.clone())
                    .map_err(|e| Error::Backend(format!("Invalid SAS token: {}", e)))?)
            },
            AzureAuthMethod::ManagedIdentity { account_name, client_id } => {
                let credential: Arc<dyn TokenCredential> = if let Some(client_id) = client_id {
                    Arc::new(DefaultAzureCredential::with_client_id(client_id.clone()))
                } else {
                    Arc::new(DefaultAzureCredential::default())
                };
                Ok(StorageCredentials::token_credential(credential))
            },
            AzureAuthMethod::ServicePrincipal { 
                account_name: _, 
                client_id, 
                client_secret, 
                tenant_id 
            } => {
                let credential = ClientSecretCredential::new(
                    tenant_id.clone(),
                    client_id.clone(),
                    client_secret.clone(),
                );
                Ok(StorageCredentials::token_credential(Arc::new(credential)))
            },
        }
    }
    
    fn extract_account_name(auth: &AzureAuthMethod) -> String {
        match auth {
            AzureAuthMethod::ConnectionString(conn_str) => {
                // Parse account name from connection string
                conn_str
                    .split(';')
                    .find(|part| part.starts_with("AccountName="))
                    .and_then(|part| part.strip_prefix("AccountName="))
                    .unwrap_or("unknown")
                    .to_string()
            },
            AzureAuthMethod::SasToken { account_name, .. } |
            AzureAuthMethod::ManagedIdentity { account_name, .. } |
            AzureAuthMethod::ServicePrincipal { account_name, .. } => account_name.clone(),
        }
    }
    
    async fn ensure_container_exists(&self) -> Result<()> {
        let container_client = self.client.container_client(&self.config.container);
        
        match container_client.get_properties().await {
            Ok(_) => Ok(()),
            Err(_) => {
                // Container doesn't exist, try to create it
                container_client
                    .create()
                    .await
                    .map_err(|e| Error::Backend(format!("Failed to create container: {}", e)))?;
                Ok(())
            }
        }
    }
    
    fn full_blob_name(&self, path: &str) -> String {
        if self.config.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.config.prefix, path)
        }
    }
    
    fn blob_client(&self, path: &str) -> BlobClient {
        let container_client = self.client.container_client(&self.config.container);
        container_client.blob_client(self.full_blob_name(path))
    }
    
    pub async fn set_blob_tier(&self, path: &str, tier: AccessTier) -> Result<()> {
        let blob_client = self.blob_client(path);
        
        retry_with_backoff(&self.retry_config, "azure_set_blob_tier", || async {
            blob_client.set_tier(tier).await
                .map_err(|e| Error::Backend(format!("Failed to set blob tier: {:?}", e)))
        }).await?;
        
        Ok(())
    }
    
    pub async fn get_blob_metadata(&self, path: &str) -> Result<std::collections::HashMap<String, String>> {
        let blob_client = self.blob_client(path);
        
        let properties = retry_with_backoff(&self.retry_config, "azure_get_metadata", || async {
            blob_client.get_properties().await
                .map_err(|e| Error::Backend(format!("Failed to get blob properties: {:?}", e)))
        }).await?;
        
        Ok(properties.blob.metadata)
    }
    
    pub async fn multipart_upload(&self, path: &str, data: Bytes) -> Result<()> {
        let blob_client = self.blob_client(path);
        
        if data.len() <= self.config.chunk_size {
            // Single upload for small files
            retry_with_backoff(&self.retry_config, "azure_single_upload", || async {
                blob_client.put_block_blob(data.clone()).await
                    .map_err(|e| Error::Backend(format!("Failed to upload blob: {:?}", e)))
            }).await?;
        } else {
            // Multipart upload for large files
            let chunks: Vec<_> = data
                .chunks(self.config.chunk_size)
                .enumerate()
                .map(|(i, chunk)| (format!("{:08}", i), Bytes::copy_from_slice(chunk)))
                .collect();
            
            // Upload blocks
            for (block_id, chunk_data) in &chunks {
                retry_with_backoff(&self.retry_config, "azure_upload_block", || async {
                    blob_client.put_block(block_id.clone(), chunk_data.clone()).await
                        .map_err(|e| Error::Backend(format!("Failed to upload block: {:?}", e)))
                }).await?;
            }
            
            // Commit blocks
            let block_list: Vec<_> = chunks.iter().map(|(id, _)| id.clone()).collect();
            retry_with_backoff(&self.retry_config, "azure_commit_blocks", || async {
                blob_client.put_block_list(block_list.clone()).await
                    .map_err(|e| Error::Backend(format!("Failed to commit blocks: {:?}", e)))
            }).await?;
        }
        
        // Set access tier if specified
        if let Some(tier) = self.config.access_tier {
            self.set_blob_tier(path, tier).await?;
        }
        
        Ok(())
    }
}

#[async_trait]
impl Backend for AzureBlobBackend {
    async fn init(&self) -> Result<()> {
        self.ensure_container_exists().await
    }
    
    async fn exists(&self, path: &str) -> Result<bool> {
        let blob_client = self.blob_client(path);
        
        match retry_with_backoff(&self.retry_config, "azure_exists", || async {
            blob_client.get_properties().await
                .map_err(|e| Error::Backend(format!("Failed to check existence: {:?}", e)))
        }).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    async fn read(&self, path: &str) -> Result<Bytes> {
        let blob_client = self.blob_client(path);
        
        let response = retry_with_backoff(&self.retry_config, "azure_read", || async {
            blob_client.get().await
                .map_err(|e| Error::Backend(format!("Failed to read blob {}: {:?}", path, e)))
        }).await?;
        
        let data = response.data.collect().await
            .map_err(|e| Error::Backend(format!("Failed to collect blob data: {}", e)))?;
        
        Ok(data)
    }
    
    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        self.multipart_upload(path, data).await
    }
    
    async fn delete(&self, path: &str) -> Result<()> {
        let blob_client = self.blob_client(path);
        
        retry_with_backoff(&self.retry_config, "azure_delete", || async {
            blob_client.delete().await
                .map_err(|e| Error::Backend(format!("Failed to delete blob {}: {:?}", path, e)))
        }).await?;
        
        Ok(())
    }
    
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let container_client = self.client.container_client(&self.config.container);
        let full_prefix = self.full_blob_name(prefix);
        
        let mut results = Vec::new();
        let mut stream = container_client.list_blobs().prefix(full_prefix).into_stream();
        
        while let Some(response) = stream.next().await {
            let response = response
                .map_err(|e| Error::Backend(format!("Failed to list blobs: {}", e)))?;
            
            for blob in response.blobs.blobs() {
                if let BlobItem::Blob(blob_item) = blob {
                    let path = if self.config.prefix.is_empty() {
                        blob_item.name.clone()
                    } else {
                        blob_item.name
                            .strip_prefix(&format!("{}/", self.config.prefix))
                            .unwrap_or(&blob_item.name)
                            .to_string()
                    };
                    results.push(path);
                }
            }
        }
        
        Ok(results)
    }
    
    async fn stat(&self, path: &str) -> Result<ObjectInfo> {
        let blob_client = self.blob_client(path);
        
        let properties = retry_with_backoff(&self.retry_config, "azure_stat", || async {
            blob_client.get_properties().await
                .map_err(|e| Error::Backend(format!("Failed to stat blob {}: {:?}", path, e)))
        }).await?;
        
        let size = properties.blob.properties.content_length;
        let modified = properties.blob.properties.last_modified;
        
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