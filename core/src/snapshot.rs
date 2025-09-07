use crate::{Result, Error, ChunkID, SnapshotID, TreeNode};
use crate::crypto::Encryptor;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use bytes::Bytes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: SnapshotID,
    pub parent: Option<SnapshotID>,
    pub tree: ChunkID,
    pub paths: Vec<PathBuf>,
    pub hostname: String,
    pub username: String,
    pub time: DateTime<Utc>,
    pub tags: Vec<String>,
    pub excludes: Vec<String>,
}

impl Snapshot {
    pub fn new(paths: Vec<PathBuf>, tree: ChunkID) -> Self {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        
        let username = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());
        
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            parent: None,
            tree,
            paths,
            hostname,
            username,
            time: Utc::now(),
            tags: Vec::new(),
            excludes: Vec::new(),
        }
    }
    
    pub fn with_parent(mut self, parent: SnapshotID) -> Self {
        self.parent = Some(parent);
        self
    }
    
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
    
    pub fn with_excludes(mut self, excludes: Vec<String>) -> Self {
        self.excludes = excludes;
        self
    }

    pub fn serialize(&self, encryptor: &Encryptor) -> Result<Bytes> {
        let json_data = serde_json::to_vec(self)
            .map_err(|e| Error::Other(format!("Failed to serialize snapshot: {}", e)))?;
        let encrypted_data = encryptor.encrypt(&json_data)?;
        Ok(Bytes::from(encrypted_data))
    }

    pub fn deserialize(data: &[u8], encryptor: &Encryptor) -> Result<Self> {
        let decrypted_data = encryptor.decrypt(data)?;
        serde_json::from_slice(&decrypted_data)
            .map_err(|e| Error::Other(format!("Failed to deserialize snapshot: {}", e)))
    }

    pub fn short_id(&self) -> String {
        self.id.chars().take(8).collect()
    }

    pub fn summary(&self) -> String {
        format!("{} - {} paths on {} at {}", 
            self.short_id(),
            self.paths.len(),
            self.hostname,
            self.time.format("%Y-%m-%d %H:%M:%S UTC")
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tree {
    pub nodes: Vec<TreeNode>,
}

impl Tree {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }
    
    pub fn add_node(&mut self, node: TreeNode) {
        self.nodes.push(node);
    }
    
    pub fn serialize(&self, encryptor: &Encryptor) -> Result<Bytes> {
        let json_data = serde_json::to_vec(self)
            .map_err(|e| Error::Other(format!("Failed to serialize tree: {}", e)))?;
        let encrypted_data = encryptor.encrypt(&json_data)?;
        Ok(Bytes::from(encrypted_data))
    }
    
    pub fn deserialize(data: &[u8], encryptor: &Encryptor) -> Result<Self> {
        let decrypted_data = encryptor.decrypt(data)?;
        serde_json::from_slice(&decrypted_data)
            .map_err(|e| Error::Other(format!("Failed to deserialize tree: {}", e)))
    }

    pub fn find_node(&self, path: &str) -> Option<&TreeNode> {
        self.nodes.iter().find(|node| node.name == path)
    }

    pub fn total_size(&self) -> u64 {
        self.nodes.iter().map(|node| node.size).sum()
    }

    pub fn file_count(&self) -> usize {
        self.nodes.iter().filter(|node| node.is_file()).count()
    }

    pub fn dir_count(&self) -> usize {
        self.nodes.iter().filter(|node| node.is_dir()).count()
    }
}

#[derive(Debug)]
pub struct SnapshotManager {
    snapshots: std::collections::HashMap<SnapshotID, Snapshot>,
}

impl SnapshotManager {
    pub fn new() -> Self {
        Self {
            snapshots: std::collections::HashMap::new(),
        }
    }

    pub fn add_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshots.insert(snapshot.id.clone(), snapshot);
    }

    pub fn get_snapshot(&self, id: &SnapshotID) -> Option<&Snapshot> {
        self.snapshots.get(id)
    }

    pub fn list_snapshots(&self) -> Vec<&Snapshot> {
        let mut snapshots: Vec<_> = self.snapshots.values().collect();
        snapshots.sort_by(|a, b| b.time.cmp(&a.time)); // Most recent first
        snapshots
    }

    pub fn find_snapshots_by_hostname(&self, hostname: &str) -> Vec<&Snapshot> {
        let mut snapshots: Vec<_> = self.snapshots.values()
            .filter(|s| s.hostname == hostname)
            .collect();
        snapshots.sort_by(|a, b| b.time.cmp(&a.time));
        snapshots
    }

    pub fn find_snapshots_by_path(&self, path: &std::path::Path) -> Vec<&Snapshot> {
        let mut snapshots: Vec<_> = self.snapshots.values()
            .filter(|s| s.paths.iter().any(|p| p == path))
            .collect();
        snapshots.sort_by(|a, b| b.time.cmp(&a.time));
        snapshots
    }

    pub fn find_snapshots_by_tag(&self, tag: &str) -> Vec<&Snapshot> {
        let mut snapshots: Vec<_> = self.snapshots.values()
            .filter(|s| s.tags.contains(&tag.to_string()))
            .collect();
        snapshots.sort_by(|a, b| b.time.cmp(&a.time));
        snapshots
    }

    pub fn remove_snapshot(&mut self, id: &SnapshotID) -> Option<Snapshot> {
        self.snapshots.remove(id)
    }

    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }
}

use hostname;