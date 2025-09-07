use crate::{Result, ChunkID, PackID, ChunkMetadata};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::fs;
use std::path::Path;

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

pub struct IndexManager {
    indices: Vec<Index>,
    master_index: Index,
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