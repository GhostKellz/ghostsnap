//! SFTP backend for remote storage over SSH.
//!
//! This backend provides storage over SSH using the SFTP protocol.
//! Note: This is a placeholder implementation. For production use,
//! consider using the rclone backend with sftp remote type.

use crate::backend::{Backend, BackendType, ObjectInfo};
use async_trait::async_trait;
use bytes::Bytes;
use ghostsnap_core::{Error, Result};
use std::path::PathBuf;

/// SFTP authentication method
#[derive(Debug, Clone)]
pub enum SftpAuth {
    /// Password authentication
    Password(String),
    /// SSH key file authentication
    KeyFile {
        path: PathBuf,
        passphrase: Option<String>,
    },
    /// SSH agent authentication
    Agent,
}

/// SFTP backend configuration
#[derive(Debug, Clone)]
pub struct SftpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SftpAuth,
    pub base_path: String,
}

impl Default for SftpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SftpAuth::Agent,
            base_path: "/backup".to_string(),
        }
    }
}

/// SFTP backend (placeholder implementation).
///
/// For production SFTP support, use the RcloneBackend with an sftp remote:
/// ```ignore
/// let backend = RcloneBackend::new("mysftp", "/backups");
/// ```
pub struct SftpBackend {
    #[allow(dead_code)]
    config: SftpConfig,
}

impl SftpBackend {
    pub fn new(config: SftpConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Backend for SftpBackend {
    async fn init(&self) -> Result<()> {
        Err(Error::Backend(
            "SFTP backend is a placeholder. Use RcloneBackend with sftp remote instead."
                .to_string(),
        ))
    }

    async fn exists(&self, _path: &str) -> Result<bool> {
        Err(Error::Backend("SFTP backend not implemented".to_string()))
    }

    async fn read(&self, _path: &str) -> Result<Bytes> {
        Err(Error::Backend("SFTP backend not implemented".to_string()))
    }

    async fn write(&self, _path: &str, _data: Bytes) -> Result<()> {
        Err(Error::Backend("SFTP backend not implemented".to_string()))
    }

    async fn delete(&self, _path: &str) -> Result<()> {
        Err(Error::Backend("SFTP backend not implemented".to_string()))
    }

    async fn list(&self, _prefix: &str) -> Result<Vec<String>> {
        Err(Error::Backend("SFTP backend not implemented".to_string()))
    }

    async fn stat(&self, _path: &str) -> Result<ObjectInfo> {
        Err(Error::Backend("SFTP backend not implemented".to_string()))
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Sftp
    }
}
