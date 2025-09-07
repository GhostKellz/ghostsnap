use crate::backend::{Backend, BackendType, ObjectInfo};
use async_trait::async_trait;
use bytes::Bytes;
use ghostsnap_core::{Error, Result};
use serde::{Deserialize, Serialize};

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

#[async_trait]
impl Backend for AzureSimpleBackend {
    async fn init(&self) -> Result<()> {
        // Placeholder for Azure initialization
        tracing::info!("Azure backend initialized (placeholder)");
        Ok(())
    }
    
    async fn exists(&self, _path: &str) -> Result<bool> {
        // Placeholder implementation
        Ok(false)
    }
    
    async fn read(&self, _path: &str) -> Result<Bytes> {
        Err(Error::Other("Azure backend not fully implemented".to_string()))
    }
    
    async fn write(&self, _path: &str, _data: Bytes) -> Result<()> {
        Err(Error::Other("Azure backend not fully implemented".to_string()))
    }
    
    async fn delete(&self, _path: &str) -> Result<()> {
        Err(Error::Other("Azure backend not fully implemented".to_string()))
    }
    
    async fn list(&self, _prefix: &str) -> Result<Vec<String>> {
        Ok(vec![])
    }
    
    async fn stat(&self, _path: &str) -> Result<ObjectInfo> {
        Err(Error::Other("Azure backend not fully implemented".to_string()))
    }
    
    fn backend_type(&self) -> BackendType {
        BackendType::Azure
    }
}