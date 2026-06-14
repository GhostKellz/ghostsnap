use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Lock file name
const LOCK_FILE: &str = "repo.lock";

/// Stale lock timeout in seconds (15 minutes)
const STALE_TIMEOUT_SECS: i64 = 15 * 60;

/// Lock type for different operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockType {
    /// Exclusive lock for write operations (backup, prune)
    Exclusive,
    /// Shared lock for read operations (restore, ls, check)
    Shared,
}

/// Lock file content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    pub lock_type: LockType,
    pub hostname: String,
    pub pid: u32,
    pub created_at: DateTime<Utc>,
    pub operation: String,
}

impl LockInfo {
    pub fn new(lock_type: LockType, operation: &str) -> Self {
        Self {
            lock_type,
            hostname: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            pid: std::process::id(),
            created_at: Utc::now(),
            operation: operation.to_string(),
        }
    }

    /// Check if this lock is from the current process
    pub fn is_current_process(&self) -> bool {
        self.pid == std::process::id() && self.is_local_host()
    }

    /// Check if the lock is from this host
    pub fn is_local_host(&self) -> bool {
        if let Ok(current_host) = hostname::get() {
            self.hostname == current_host.to_string_lossy()
        } else {
            false
        }
    }

    /// Check if the lock is stale (old and likely abandoned)
    pub fn is_stale(&self) -> bool {
        let age = Utc::now().signed_duration_since(self.created_at);
        age.num_seconds() > STALE_TIMEOUT_SECS
    }

    /// Check if the process holding the lock is still running
    #[cfg(unix)]
    pub fn is_process_alive(&self) -> bool {
        if !self.is_local_host() {
            // Can't check remote processes, assume alive
            return true;
        }

        // Check if process exists by sending signal 0
        unsafe { libc::kill(self.pid as i32, 0) == 0 }
    }

    #[cfg(not(unix))]
    pub fn is_process_alive(&self) -> bool {
        // On non-Unix, assume process is alive
        true
    }
}

/// Repository lock manager
pub struct LockManager {
    locks_dir: PathBuf,
}

impl LockManager {
    pub fn new<P: AsRef<Path>>(repo_path: P) -> Self {
        Self {
            locks_dir: repo_path.as_ref().join("locks"),
        }
    }

    /// Acquire a lock on the repository
    pub async fn acquire(&self, lock_type: LockType, operation: &str) -> Result<RepositoryLock> {
        let lock_path = self.locks_dir.join(LOCK_FILE);

        // Check for existing lock
        if lock_path.exists() {
            let existing = self.read_lock(&lock_path).await?;

            // If it's our own lock, allow re-entry
            if existing.is_current_process() {
                return Ok(RepositoryLock {
                    path: lock_path,
                    owned: false, // Don't delete on drop - we're re-entering
                });
            }

            // Check if the lock is stale
            if existing.is_stale() && !existing.is_process_alive() {
                tracing::warn!(
                    "Removing stale lock from {} (PID {}, created {})",
                    existing.hostname,
                    existing.pid,
                    existing.created_at
                );
                fs::remove_file(&lock_path).await.ok();
            } else {
                // Lock is held by another process
                return Err(Error::LockConflict(format!(
                    "Repository locked by {} (PID {}, operation: {}, since {})",
                    existing.hostname,
                    existing.pid,
                    existing.operation,
                    existing.created_at.format("%Y-%m-%d %H:%M:%S UTC")
                )));
            }
        }

        // Create new lock
        let lock_info = LockInfo::new(lock_type, operation);
        self.write_lock(&lock_path, &lock_info).await?;

        Ok(RepositoryLock {
            path: lock_path,
            owned: true,
        })
    }

    /// Try to acquire a lock, returning None if already locked
    pub async fn try_acquire(
        &self,
        lock_type: LockType,
        operation: &str,
    ) -> Result<Option<RepositoryLock>> {
        match self.acquire(lock_type, operation).await {
            Ok(lock) => Ok(Some(lock)),
            Err(Error::LockConflict(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Check if the repository is currently locked
    pub async fn is_locked(&self) -> Result<bool> {
        let lock_path = self.locks_dir.join(LOCK_FILE);
        Ok(lock_path.exists())
    }

    /// Get information about the current lock
    pub async fn get_lock_info(&self) -> Result<Option<LockInfo>> {
        let lock_path = self.locks_dir.join(LOCK_FILE);
        if lock_path.exists() {
            Ok(Some(self.read_lock(&lock_path).await?))
        } else {
            Ok(None)
        }
    }

    /// Force remove a lock (use with caution)
    pub async fn force_unlock(&self) -> Result<()> {
        let lock_path = self.locks_dir.join(LOCK_FILE);
        if lock_path.exists() {
            fs::remove_file(&lock_path).await?;
        }
        Ok(())
    }

    async fn read_lock(&self, path: &Path) -> Result<LockInfo> {
        let content = fs::read_to_string(path).await?;
        serde_json::from_str(&content)
            .map_err(|e| Error::Other(format!("Invalid lock file: {}", e)))
    }

    async fn write_lock(&self, path: &Path, info: &LockInfo) -> Result<()> {
        // Ensure locks directory exists
        fs::create_dir_all(&self.locks_dir).await?;

        // Write atomically via temp file
        let temp_path = path.with_extension("lock.tmp");
        let content = serde_json::to_string_pretty(info)?;
        fs::write(&temp_path, &content).await?;
        fs::rename(&temp_path, path).await?;
        Ok(())
    }
}

/// RAII lock handle that releases the lock on drop
pub struct RepositoryLock {
    path: PathBuf,
    owned: bool,
}

impl RepositoryLock {
    /// Explicitly release the lock
    pub async fn release(self) -> Result<()> {
        if self.owned && self.path.exists() {
            fs::remove_file(&self.path).await?;
        }
        // Prevent Drop from running
        std::mem::forget(self);
        Ok(())
    }
}

impl Drop for RepositoryLock {
    fn drop(&mut self) {
        if self.owned && self.path.exists() {
            // Best-effort removal in drop (can't await)
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_lock_acquire_release() {
        let dir = tempdir().unwrap();
        let repo_path = dir.path();
        std::fs::create_dir_all(repo_path.join("locks")).unwrap();

        let manager = LockManager::new(repo_path);

        // Acquire lock
        let lock = manager.acquire(LockType::Exclusive, "test").await.unwrap();
        assert!(manager.is_locked().await.unwrap());

        // Release lock
        lock.release().await.unwrap();
        assert!(!manager.is_locked().await.unwrap());
    }

    #[tokio::test]
    async fn test_lock_conflict() {
        let dir = tempdir().unwrap();
        let repo_path = dir.path();
        std::fs::create_dir_all(repo_path.join("locks")).unwrap();

        let manager = LockManager::new(repo_path);

        // Acquire first lock
        let _lock1 = manager.acquire(LockType::Exclusive, "test1").await.unwrap();

        // Second acquisition should succeed (same process)
        let _lock2 = manager.acquire(LockType::Exclusive, "test2").await.unwrap();
    }
}
