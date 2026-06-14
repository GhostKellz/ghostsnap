//! Rclone backend wrapper.
//!
//! This backend wraps the rclone command-line tool to provide access to
//! 40+ cloud storage providers. Requires rclone to be installed and configured.
//!
//! # Example
//!
//! ```no_run
//! use ghostsnap_backends::RcloneBackend;
//!
//! let backend = RcloneBackend::new("myremote", "/backups/ghostsnap");
//! ```

use crate::backend::{Backend, BackendType, ObjectInfo};
use async_trait::async_trait;
use bytes::Bytes;
use ghostsnap_core::{Error, Result};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Rclone backend that wraps the rclone CLI tool.
pub struct RcloneBackend {
    /// The rclone remote name (e.g., "myremote" for "myremote:/path")
    remote: String,
    /// Base path within the remote
    base_path: String,
    /// Path to rclone binary (default: "rclone")
    rclone_path: String,
    /// Additional rclone flags
    extra_flags: Vec<String>,
}

impl RcloneBackend {
    /// Creates a new rclone backend.
    ///
    /// # Arguments
    ///
    /// * `remote` - The rclone remote name (as configured in rclone)
    /// * `base_path` - Base path within the remote
    pub fn new(remote: impl Into<String>, base_path: impl Into<String>) -> Self {
        Self {
            remote: remote.into(),
            base_path: base_path.into(),
            rclone_path: "rclone".to_string(),
            extra_flags: Vec::new(),
        }
    }

    /// Sets a custom path to the rclone binary.
    pub fn with_rclone_path(mut self, path: impl Into<String>) -> Self {
        self.rclone_path = path.into();
        self
    }

    /// Adds extra flags to pass to rclone commands.
    pub fn with_flags(mut self, flags: Vec<String>) -> Self {
        self.extra_flags = flags;
        self
    }

    /// Returns the full rclone path for a given relative path.
    fn full_path(&self, path: &str) -> String {
        let base = self.base_path.trim_end_matches('/');
        if base.is_empty() {
            format!("{}:{}", self.remote, path)
        } else {
            format!("{}:{}/{}", self.remote, base, path)
        }
    }

    /// Runs an rclone command and returns (success, stdout, stderr).
    async fn run_rclone(&self, args: &[&str]) -> Result<(bool, Vec<u8>, String)> {
        let mut cmd = Command::new(&self.rclone_path);

        for arg in args {
            cmd.arg(arg);
        }

        for flag in &self.extra_flags {
            cmd.arg(flag);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Backend(format!("Failed to spawn rclone: {}", e)))?;

        let mut stdout = Vec::new();
        let mut stderr = String::new();

        if let Some(ref mut stdout_pipe) = child.stdout {
            stdout_pipe
                .read_to_end(&mut stdout)
                .await
                .map_err(|e| Error::Backend(format!("Failed to read rclone stdout: {}", e)))?;
        }

        if let Some(ref mut stderr_pipe) = child.stderr {
            use tokio::io::AsyncBufReadExt;
            use tokio::io::BufReader;
            let mut reader = BufReader::new(stderr_pipe);
            reader.read_line(&mut stderr).await.ok();
        }

        let status = child
            .wait()
            .await
            .map_err(|e| Error::Backend(format!("Failed to wait for rclone: {}", e)))?;

        Ok((status.success(), stdout, stderr))
    }
}

#[async_trait]
impl Backend for RcloneBackend {
    async fn init(&self) -> Result<()> {
        // Verify rclone is available and remote is configured
        let (success, _, stderr) = self.run_rclone(&["version"]).await?;

        if !success {
            return Err(Error::Backend(format!(
                "rclone not available or not working: {}",
                stderr
            )));
        }

        // Create base directory if needed
        let path = self.full_path("");
        let (success, _, stderr) = self.run_rclone(&["mkdir", &path]).await?;

        if !success && !stderr.contains("directory not empty") {
            return Err(Error::Backend(format!(
                "Failed to create base directory: {}",
                stderr
            )));
        }

        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let full_path = self.full_path(path);
        let (success, stdout, _) = self.run_rclone(&["lsf", &full_path]).await?;

        // rclone lsf returns empty output for non-existent files
        Ok(success && !stdout.is_empty())
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        let full_path = self.full_path(path);
        let (success, stdout, stderr) = self.run_rclone(&["cat", &full_path]).await?;

        if !success {
            return Err(Error::Backend(format!(
                "Failed to read {}: {}",
                path, stderr
            )));
        }

        Ok(Bytes::from(stdout))
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let full_path = self.full_path(path);

        // Write to a temp file first, then rclone copy it
        let temp_dir = tempfile::tempdir()
            .map_err(|e| Error::Backend(format!("Failed to create temp dir: {}", e)))?;

        let temp_file = temp_dir.path().join("data");
        tokio::fs::write(&temp_file, &data)
            .await
            .map_err(|e| Error::Backend(format!("Failed to write temp file: {}", e)))?;

        let temp_path = temp_file.to_string_lossy();
        let (success, _, stderr) = self.run_rclone(&["copyto", &temp_path, &full_path]).await?;

        if !success {
            return Err(Error::Backend(format!(
                "Failed to write {}: {}",
                path, stderr
            )));
        }

        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full_path = self.full_path(path);
        let (success, _, stderr) = self.run_rclone(&["deletefile", &full_path]).await?;

        if !success {
            return Err(Error::Backend(format!(
                "Failed to delete {}: {}",
                path, stderr
            )));
        }

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let full_path = self.full_path(prefix);
        let (success, stdout, stderr) =
            self.run_rclone(&["lsf", "--recursive", &full_path]).await?;

        if !success {
            // Empty directory is not an error
            if stderr.contains("directory not found") {
                return Ok(Vec::new());
            }
            return Err(Error::Backend(format!(
                "Failed to list {}: {}",
                prefix, stderr
            )));
        }

        let files: Vec<String> = String::from_utf8_lossy(&stdout)
            .lines()
            .map(|line| {
                // Remove trailing slashes from directories
                line.trim_end_matches('/').to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();

        Ok(files)
    }

    async fn stat(&self, path: &str) -> Result<ObjectInfo> {
        let full_path = self.full_path(path);
        let (success, stdout, stderr) = self.run_rclone(&["lsjson", &full_path]).await?;

        if !success {
            return Err(Error::Backend(format!(
                "Failed to stat {}: {}",
                path, stderr
            )));
        }

        // Parse JSON output
        let json: Vec<serde_json::Value> = serde_json::from_slice(&stdout)
            .map_err(|e| Error::Backend(format!("Failed to parse rclone output: {}", e)))?;

        if json.is_empty() {
            return Err(Error::Backend(format!("File not found: {}", path)));
        }

        let item = &json[0];
        let size = item["Size"].as_u64().unwrap_or(0);
        let mod_time = item["ModTime"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        Ok(ObjectInfo {
            path: path.to_string(),
            size,
            modified: mod_time,
        })
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Rclone
    }
}
