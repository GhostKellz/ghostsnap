use crate::{Result, ChunkID, PackID, ChunkMetadata};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::fs;
use std::path::Path;

/// An index that maps chunk IDs to their physical locations in pack files.
///
/// The index is the critical data structure for fast chunk lookups during backup
/// (for deduplication) and restore operations. It maintains:
///
/// - A mapping of chunk IDs to their metadata (pack ID, offset, length)
/// - Information about pack files (size, chunk count)
///
/// # Examples
///
/// ```
/// use ghostsnap_core::index::Index;
///
/// let mut index = Index::new();
/// // Add chunks and packs during backup...
/// assert!(index.chunks.is_empty());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub chunks: HashMap<ChunkID, ChunkMetadata>,
    pub packs: HashMap<PackID, PackInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackInfo {
    pub id: PackID,
    pub size: u64,
    pub chunk_count: u32,
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

impl Index {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            packs: HashMap::new(),
        }
    }
    
    pub fn add_chunk(&mut self, metadata: ChunkMetadata) {
        self.chunks.insert(metadata.id, metadata);
    }
    
    pub fn add_pack(&mut self, info: PackInfo) {
        self.packs.insert(info.id.clone(), info);
    }
    
    pub fn has_chunk(&self, id: &ChunkID) -> bool {
        self.chunks.contains_key(id)
    }
    
    pub fn get_chunk(&self, id: &ChunkID) -> Option<&ChunkMetadata> {
        self.chunks.get(id)
    }
    
    pub fn merge(&mut self, other: Index) {
        self.chunks.extend(other.chunks);
        self.packs.extend(other.packs);
    }
    
    pub async fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let data = serde_json::to_vec(self)?;
        fs::write(path, data).await?;
        Ok(())
    }
    
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let data = fs::read(path).await?;
        Ok(serde_json::from_slice(&data)?)
    }
}

/// Manages multiple index files and provides a unified view.
///
/// The `IndexManager` maintains both individual index files and a master index
/// for fast lookups. It handles index merging, compaction, and provides an
/// interface for chunk lookups across all indices.
///
/// # Examples
///
/// ```
/// use ghostsnap_core::index::IndexManager;
///
/// let mut manager = IndexManager::new();
/// // Add indices as backups are created...
/// ```
pub struct IndexManager {
    indices: Vec<Index>,
    master_index: Index,
}

impl Default for IndexManager {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexManager {
    pub fn new() -> Self {
        Self {
            indices: Vec::new(),
            master_index: Index::new(),
        }
    }
    
    pub fn add_index(&mut self, index: Index) {
        self.master_index.merge(index.clone());
        self.indices.push(index);
    }
    
    pub fn has_chunk(&self, id: &ChunkID) -> bool {
        self.master_index.has_chunk(id)
    }
    
    pub fn get_chunk(&self, id: &ChunkID) -> Option<&ChunkMetadata> {
        self.master_index.get_chunk(id)
    }
    
    pub fn compact(&mut self) -> Index {
        let mut compacted = Index::new();
        for index in &self.indices {
            compacted.merge(index.clone());
        }
        self.indices.clear();
        self.indices.push(compacted.clone());
        self.master_index = compacted.clone();
        compacted
    }
}