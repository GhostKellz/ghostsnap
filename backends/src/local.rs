use crate::backend::{Backend, BackendType, ObjectInfo};
use async_trait::async_trait;
use bytes::Bytes;
use ghostsnap_core::{Error, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use chrono::{DateTime, Utc};

pub struct LocalBackend {
    base_path: PathBuf,
}

impl LocalBackend {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }
    
    fn full_path(&self, path: &str) -> PathBuf {
        self.base_path.join(path)
    }
}

#[async_trait]
impl Backend for LocalBackend {
    async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.base_path).await?;
        Ok(())
    }
    
    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.full_path(path).exists())
    }
    
    async fn read(&self, path: &str) -> Result<Bytes> {
        let full_path = self.full_path(path);
        let data = fs::read(&full_path).await
            .map_err(|e| Error::Backend(format!("Failed to read {}: {}", path, e)))?;
        Ok(Bytes::from(data))
    }
    
    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let full_path = self.full_path(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&full_path, &data).await
            .map_err(|e| Error::Backend(format!("Failed to write {}: {}", path, e)))?;
        Ok(())
    }
    
    async fn delete(&self, path: &str) -> Result<()> {
        let full_path = self.full_path(path);
        if full_path.is_file() {
            fs::remove_file(&full_path).await
                .map_err(|e| Error::Backend(format!("Failed to delete {}: {}", path, e)))?;
        } else if full_path.is_dir() {
            fs::remove_dir_all(&full_path).await
                .map_err(|e| Error::Backend(format!("Failed to delete {}: {}", path, e)))?;
        }
        Ok(())
    }
    
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let full_path = self.full_path(prefix);
        let mut results = Vec::new();
        
        if full_path.exists() && full_path.is_dir() {
            let mut entries = fs::read_dir(&full_path).await?;
            while let Some(entry) = entries.next_entry().await? {
                if let Some(name) = entry.file_name().to_str() {
                    results.push(format!("{}/{}", prefix, name));
                }
            }
        }
        
        Ok(results)
    }
    
    async fn stat(&self, path: &str) -> Result<ObjectInfo> {
        let full_path = self.full_path(path);
        let metadata = fs::metadata(&full_path).await
            .map_err(|e| Error::Backend(format!("Failed to stat {}: {}", path, e)))?;
        
        let modified = metadata.modified()
            .map_err(|e| Error::Backend(format!("Failed to get modified time: {}", e)))?;
        
        let modified_dt: DateTime<Utc> = modified.into();
        
        Ok(ObjectInfo {
            path: path.to_string(),
            size: metadata.len(),
            modified: modified_dt,
        })
    }
    
    fn backend_type(&self) -> BackendType {
        BackendType::Local
    }
}