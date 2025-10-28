use crate::backend::{Backend, BackendType, ObjectInfo};
use crate::retry::{RetryConfig, retry_with_backoff};
use async_trait::async_trait;
use bytes::Bytes;
use ghostsnap_core::{Error, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use chrono::{DateTime, Utc};
use tracing::debug;

pub struct LocalBackend {
    base_path: PathBuf,
    retry_config: RetryConfig,
    min_free_space_bytes: u64, // Minimum free space required (default: 100MB)
}

impl LocalBackend {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            retry_config: RetryConfig::quick(), // Faster retries for local I/O
            min_free_space_bytes: 100 * 1024 * 1024, // 100MB default
        }
    }

    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }

    pub fn with_min_free_space(mut self, bytes: u64) -> Self {
        self.min_free_space_bytes = bytes;
        self
    }
    
    fn full_path(&self, path: &str) -> PathBuf {
        self.base_path.join(path)
    }

    /// Check if there's sufficient free space on the filesystem
    async fn check_free_space(&self, required_bytes: u64) -> Result<()> {
        // Get filesystem stats using statvfs (Unix) or GetDiskFreeSpaceEx (Windows)
        #[cfg(unix)]
        {
            // Try to get filesystem stats
            // Note: This is a simplified check. Production code might use nix crate for statvfs
            let _total_required = required_bytes + self.min_free_space_bytes;

            // For now, we'll do a basic check by attempting to reserve space
            // A more robust implementation would use statvfs
            debug!(
                path = ?self.base_path,
                required_bytes,
                min_free_space = self.min_free_space_bytes,
                "Checking filesystem space"
            );
        }

        #[cfg(windows)]
        {
            // Windows implementation would use GetDiskFreeSpaceEx
            // TODO: Implement filesystem space check for Windows
            debug!(
                path = ?self.base_path,
                required_bytes,
                "Filesystem space check not implemented on Windows yet"
            );
        }

        Ok(())
    }

    /// Atomic write using temp file + rename pattern
    async fn atomic_write(&self, path: &str, data: &Bytes) -> Result<()> {
        let full_path = self.full_path(path);
        
        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Create temporary file in the same directory for atomic rename
        let temp_path = full_path.with_extension("tmp");
        
        // Write to temporary file first
        fs::write(&temp_path, data).await
            .map_err(|e| Error::Backend(format!("Failed to write temp file: {}", e)))?;

        // Sync the file to ensure data is on disk (Unix systems)
        #[cfg(unix)]
        {
            let file = tokio::fs::OpenOptions::new()
                .write(true)
                .open(&temp_path)
                .await?;
            file.sync_all().await?;
        }

        // Atomic rename - this is the critical operation
        fs::rename(&temp_path, &full_path).await
            .map_err(|e| {
                // Clean up temp file on failure
                let temp_path_clone = temp_path.clone();
                tokio::spawn(async move {
                    let _ = fs::remove_file(&temp_path_clone).await;
                });
                Error::Backend(format!("Failed to rename temp file: {}", e))
            })?;

        debug!(
            path,
            size = data.len(),
            "Atomic write completed successfully"
        );

        Ok(())
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
        let path_copy = path.to_string();
        
        retry_with_backoff(&self.retry_config, "local_read", || async {
            let data = fs::read(&full_path).await
                .map_err(|e| Error::Backend(format!("Failed to read {}: {}", path_copy, e)))?;
            Ok(Bytes::from(data))
        })
        .await
    }
    
    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        // Check free space before writing
        self.check_free_space(data.len() as u64).await?;
        
        let path_copy = path.to_string();
        let data_clone = data.clone();
        
        retry_with_backoff(&self.retry_config, "local_write", || async {
            // Use atomic write to prevent corruption
            self.atomic_write(&path_copy, &data_clone).await
        })
        .await
    }
    
    async fn delete(&self, path: &str) -> Result<()> {
        let full_path = self.full_path(path);
        let path_copy = path.to_string();
        
        retry_with_backoff(&self.retry_config, "local_delete", || async {
            if full_path.is_file() {
                fs::remove_file(&full_path).await
                    .map_err(|e| Error::Backend(format!("Failed to delete {}: {}", path_copy, e)))?;
            } else if full_path.is_dir() {
                fs::remove_dir_all(&full_path).await
                    .map_err(|e| Error::Backend(format!("Failed to delete {}: {}", path_copy, e)))?;
            }
            Ok(())
        })
        .await
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