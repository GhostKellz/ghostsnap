use async_trait::async_trait;
use ghostsnap_core::Result;
use bytes::Bytes;

#[derive(Debug, Clone)]
pub enum BackendType {
    Local,
    S3,
    Azure,
    MinIO,
    B2,
}

#[async_trait]
pub trait Backend: Send + Sync {
    async fn init(&self) -> Result<()>;
    
    async fn exists(&self, path: &str) -> Result<bool>;
    
    async fn read(&self, path: &str) -> Result<Bytes>;
    
    async fn write(&self, path: &str, data: Bytes) -> Result<()>;
    
    async fn delete(&self, path: &str) -> Result<()>;
    
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    
    async fn stat(&self, path: &str) -> Result<ObjectInfo>;
    
    fn backend_type(&self) -> BackendType;
}

#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub path: String,
    pub size: u64,
    pub modified: chrono::DateTime<chrono::Utc>,
}