use crate::crypto::Encryptor;
use crate::{ChunkID, ChunkMetadata, Error, PackID, Result};
use bloomfilter::Bloom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

/// Current index format version for schema evolution
const INDEX_VERSION: u32 = 2;

/// Bloom filter parameters - tuned for 1M chunks with 0.1% false positive rate
const BLOOM_ITEMS_COUNT: usize = 1_000_000;
const BLOOM_FP_RATE: f64 = 0.001;

/// Location of a chunk within a pack file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkLocation {
    pub pack_id: PackID,
    pub offset: u64,
    pub length: u32,
}

/// Pack file information for statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackInfo {
    pub id: PackID,
    pub size: u64,
    pub chunk_count: u32,
}

/// Consolidated index header for versioning.
#[derive(Debug, Serialize, Deserialize)]
struct IndexHeader {
    version: u32,
    chunk_count: u64,
    pack_count: u64,
}

/// Serializable index data (without bloom filter).
#[derive(Debug, Serialize, Deserialize)]
struct IndexData {
    header: IndexHeader,
    chunks: HashMap<ChunkID, ChunkLocation>,
    packs: HashMap<PackID, PackInfo>,
}

/// The main chunk index with bloom filter for fast lookups.
///
/// The index maintains:
/// - A bloom filter for O(1) chunk existence checks
/// - A HashMap mapping chunk IDs to their pack locations
/// - Pack file metadata for statistics
///
/// The bloom filter eliminates most disk reads for chunks that don't exist,
/// which is critical for deduplication during backup.
pub struct Index {
    /// Bloom filter for fast negative lookups
    bloom: Bloom<ChunkID>,
    /// Chunk ID to location mapping
    chunks: HashMap<ChunkID, ChunkLocation>,
    /// Pack metadata
    packs: HashMap<PackID, PackInfo>,
    /// Track if index has unsaved changes
    dirty: bool,
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

impl Index {
    /// Creates a new empty index.
    pub fn new() -> Self {
        Self {
            bloom: Bloom::new_for_fp_rate(BLOOM_ITEMS_COUNT, BLOOM_FP_RATE),
            chunks: HashMap::new(),
            packs: HashMap::new(),
            dirty: false,
        }
    }

    /// Creates an index with pre-allocated capacity.
    pub fn with_capacity(chunk_capacity: usize) -> Self {
        let bloom_size = chunk_capacity.max(BLOOM_ITEMS_COUNT);
        Self {
            bloom: Bloom::new_for_fp_rate(bloom_size, BLOOM_FP_RATE),
            chunks: HashMap::with_capacity(chunk_capacity),
            packs: HashMap::new(),
            dirty: false,
        }
    }

    /// Adds a chunk to the index.
    pub fn add_chunk(&mut self, chunk_id: ChunkID, location: ChunkLocation) {
        self.bloom.set(&chunk_id);
        self.chunks.insert(chunk_id, location);
        self.dirty = true;
    }

    /// Adds a chunk from full metadata.
    pub fn add_chunk_metadata(&mut self, metadata: &ChunkMetadata) {
        let location = ChunkLocation {
            pack_id: metadata.pack_id.clone(),
            offset: metadata.offset,
            length: metadata.length,
        };
        self.add_chunk(metadata.id, location);
    }

    /// Adds pack information.
    pub fn add_pack(&mut self, info: PackInfo) {
        self.packs.insert(info.id.clone(), info);
        self.dirty = true;
    }

    /// Fast bloom filter check - may have false positives but no false negatives.
    /// Use this for quick rejection before doing actual lookup.
    pub fn might_have_chunk(&self, id: &ChunkID) -> bool {
        self.bloom.check(id)
    }

    /// Definitive chunk existence check.
    pub fn has_chunk(&self, id: &ChunkID) -> bool {
        // Fast path: bloom filter says no -> definitely no
        if !self.bloom.check(id) {
            return false;
        }
        // Bloom says maybe -> check HashMap
        self.chunks.contains_key(id)
    }

    /// Gets chunk location if it exists.
    pub fn get_chunk(&self, id: &ChunkID) -> Option<&ChunkLocation> {
        if !self.bloom.check(id) {
            return None;
        }
        self.chunks.get(id)
    }

    /// Gets pack information.
    pub fn get_pack(&self, id: &PackID) -> Option<&PackInfo> {
        self.packs.get(id)
    }

    /// Returns the number of chunks in the index.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Returns the number of packs in the index.
    pub fn pack_count(&self) -> usize {
        self.packs.len()
    }

    /// Returns whether the index has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the index as clean (just saved).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Merges another index into this one.
    pub fn merge(&mut self, other: Index) {
        for (id, loc) in other.chunks {
            self.bloom.set(&id);
            self.chunks.insert(id, loc);
        }
        self.packs.extend(other.packs);
        self.dirty = true;
    }

    /// Iterates over all chunks.
    pub fn iter_chunks(&self) -> impl Iterator<Item = (&ChunkID, &ChunkLocation)> {
        self.chunks.iter()
    }

    /// Iterates over all packs.
    pub fn iter_packs(&self) -> impl Iterator<Item = (&PackID, &PackInfo)> {
        self.packs.iter()
    }

    /// Removes a chunk from the index (for pruning).
    pub fn remove_chunk(&mut self, id: &ChunkID) -> Option<ChunkLocation> {
        // Note: Can't remove from bloom filter, but that's okay -
        // it just means slightly more false positives after pruning.
        self.dirty = true;
        self.chunks.remove(id)
    }

    /// Removes a pack from the index.
    pub fn remove_pack(&mut self, id: &PackID) -> Option<PackInfo> {
        self.dirty = true;
        self.packs.remove(id)
    }

    /// Compacts the index by removing chunks not in the given set of used chunk IDs.
    /// Returns the number of chunks removed.
    pub fn compact(&mut self, used_chunks: &std::collections::HashSet<ChunkID>) -> usize {
        let original_count = self.chunks.len();

        // Remove unused chunks
        self.chunks.retain(|id, _| used_chunks.contains(id));

        let removed_count = original_count - self.chunks.len();

        if removed_count > 0 {
            // Rebuild bloom filter with remaining chunks
            self.rebuild_bloom();
            self.dirty = true;
        }

        removed_count
    }

    /// Rebuilds the bloom filter from the current chunk set.
    fn rebuild_bloom(&mut self) {
        let bloom_size = (self.chunks.len() * 2).max(BLOOM_ITEMS_COUNT);
        self.bloom = Bloom::new_for_fp_rate(bloom_size, BLOOM_FP_RATE);
        for id in self.chunks.keys() {
            self.bloom.set(id);
        }
    }

    /// Returns all chunk IDs in the index.
    pub fn all_chunk_ids(&self) -> std::collections::HashSet<ChunkID> {
        self.chunks.keys().cloned().collect()
    }

    /// Returns all pack IDs in the index.
    pub fn all_pack_ids(&self) -> Vec<PackID> {
        self.packs.keys().cloned().collect()
    }

    /// Returns chunks belonging to a specific pack.
    pub fn chunks_in_pack(&self, pack_id: &PackID) -> Vec<ChunkID> {
        self.chunks
            .iter()
            .filter(|(_, loc)| loc.pack_id == *pack_id)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Saves the index to an encrypted binary file.
    pub async fn save_encrypted<P: AsRef<Path>>(
        &self,
        path: P,
        encryptor: &Encryptor,
    ) -> Result<()> {
        let encrypted = self.to_encrypted_bytes(encryptor)?;

        // Write atomically via temp file
        let path = path.as_ref();
        let temp_path = path.with_extension("idx.tmp");
        fs::write(&temp_path, &encrypted).await?;
        fs::rename(&temp_path, path).await?;

        Ok(())
    }

    pub fn to_encrypted_bytes(&self, encryptor: &Encryptor) -> Result<Vec<u8>> {
        let data = IndexData {
            header: IndexHeader {
                version: INDEX_VERSION,
                chunk_count: self.chunks.len() as u64,
                pack_count: self.packs.len() as u64,
            },
            chunks: self.chunks.clone(),
            packs: self.packs.clone(),
        };

        // Serialize with postcard (compact binary format)
        let serialized = postcard::to_allocvec(&data)
            .map_err(|e| Error::Other(format!("Index serialization failed: {}", e)))?;

        // Encrypt
        let encrypted = encryptor.encrypt(&serialized)?;
        Ok(encrypted)
    }

    /// Loads the index from an encrypted binary file.
    pub async fn load_encrypted<P: AsRef<Path>>(path: P, encryptor: &Encryptor) -> Result<Self> {
        let encrypted = fs::read(path).await?;
        Self::from_encrypted_bytes(&encrypted, encryptor)
    }

    pub fn from_encrypted_bytes(encrypted: &[u8], encryptor: &Encryptor) -> Result<Self> {
        let serialized = encryptor.decrypt(encrypted)?;

        let data: IndexData = postcard::from_bytes(&serialized)
            .map_err(|e| Error::Other(format!("Index deserialization failed: {}", e)))?;

        if data.header.version > INDEX_VERSION {
            return Err(Error::Other(format!(
                "Index version {} is newer than supported version {}",
                data.header.version, INDEX_VERSION
            )));
        }

        // Rebuild bloom filter from chunks
        let bloom_size = (data.chunks.len() * 2).max(BLOOM_ITEMS_COUNT);
        let mut bloom = Bloom::new_for_fp_rate(bloom_size, BLOOM_FP_RATE);
        for id in data.chunks.keys() {
            bloom.set(id);
        }

        Ok(Self {
            bloom,
            chunks: data.chunks,
            packs: data.packs,
            dirty: false,
        })
    }

    /// Loads index from legacy per-file format and converts to consolidated.
    /// Used for migration from old repository format.
    pub async fn load_from_legacy_dir<P: AsRef<Path>>(index_dir: P) -> Result<Self> {
        let index_dir = index_dir.as_ref();
        let mut index = Self::new();

        let mut entries = fs::read_dir(index_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Skip the consolidated index file
            if name.ends_with(".idx") {
                continue;
            }

            // Try to parse as chunk ID (64 hex chars)
            if name.len() == 64
                && let Ok(chunk_id) = name.parse::<ChunkID>()
                && let Ok(data) = fs::read(entry.path()).await
                && let Ok(location) = serde_json::from_slice::<LegacyChunkLocation>(&data)
            {
                index.add_chunk(
                    chunk_id,
                    ChunkLocation {
                        pack_id: location.pack_id,
                        offset: location.offset,
                        length: location.length,
                    },
                );
            }
        }

        Ok(index)
    }
}

/// Legacy chunk location format (JSON) for migration.
#[derive(Deserialize)]
struct LegacyChunkLocation {
    pack_id: PackID,
    offset: u64,
    length: u32,
}

/// Number of shards for large indexes (256 = one per byte prefix)
const SHARD_COUNT: usize = 256;

/// Threshold for switching to sharded index (1 million chunks)
const SHARD_THRESHOLD: usize = 1_000_000;

/// Sharded index header for versioning.
#[derive(Debug, Serialize, Deserialize)]
struct ShardedIndexHeader {
    version: u32,
    shard_count: u32,
    total_chunks: u64,
    total_packs: u64,
}

/// A sharded index for very large repositories.
///
/// Splits chunks across multiple shard files based on the first byte
/// of the chunk ID. This allows:
/// - Parallel loading of only needed shards
/// - Memory-efficient partial loading
/// - Faster updates (smaller files to rewrite)
pub struct ShardedIndex {
    /// 256 shards, indexed by first byte of chunk ID
    shards: Vec<Index>,
    /// Pack metadata (shared across all shards)
    packs: HashMap<PackID, PackInfo>,
    /// Track if any shard has changes
    dirty: bool,
}

impl ShardedIndex {
    /// Creates a new empty sharded index.
    pub fn new() -> Self {
        let shards: Vec<Index> = (0..SHARD_COUNT).map(|_| Index::new()).collect();
        Self {
            shards,
            packs: HashMap::new(),
            dirty: false,
        }
    }

    /// Converts from a regular index to sharded.
    pub fn from_index(index: Index) -> Self {
        let mut sharded = Self::new();

        // Distribute chunks to shards
        for (chunk_id, location) in index.chunks {
            let shard_idx = chunk_id.as_bytes()[0] as usize;
            sharded.shards[shard_idx].add_chunk(chunk_id, location);
        }

        sharded.packs = index.packs;
        sharded.dirty = true;
        sharded
    }

    /// Returns shard index for a chunk ID.
    #[inline]
    fn shard_index(chunk_id: &ChunkID) -> usize {
        chunk_id.as_bytes()[0] as usize
    }

    /// Adds a chunk to the appropriate shard.
    pub fn add_chunk(&mut self, chunk_id: ChunkID, location: ChunkLocation) {
        let shard_idx = Self::shard_index(&chunk_id);
        self.shards[shard_idx].add_chunk(chunk_id, location);
        self.dirty = true;
    }

    /// Adds pack information.
    pub fn add_pack(&mut self, info: PackInfo) {
        self.packs.insert(info.id.clone(), info);
        self.dirty = true;
    }

    /// Fast bloom filter check across all shards.
    pub fn might_have_chunk(&self, id: &ChunkID) -> bool {
        let shard_idx = Self::shard_index(id);
        self.shards[shard_idx].might_have_chunk(id)
    }

    /// Definitive chunk existence check.
    pub fn has_chunk(&self, id: &ChunkID) -> bool {
        let shard_idx = Self::shard_index(id);
        self.shards[shard_idx].has_chunk(id)
    }

    /// Gets chunk location if it exists.
    pub fn get_chunk(&self, id: &ChunkID) -> Option<&ChunkLocation> {
        let shard_idx = Self::shard_index(id);
        self.shards[shard_idx].get_chunk(id)
    }

    /// Gets pack information.
    pub fn get_pack(&self, id: &PackID) -> Option<&PackInfo> {
        self.packs.get(id)
    }

    /// Returns total number of chunks across all shards.
    pub fn chunk_count(&self) -> usize {
        self.shards.iter().map(|s| s.chunk_count()).sum()
    }

    /// Returns the number of packs.
    pub fn pack_count(&self) -> usize {
        self.packs.len()
    }

    /// Returns whether any shard has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty || self.shards.iter().any(|s| s.is_dirty())
    }

    /// Marks all shards as clean.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
        for shard in &mut self.shards {
            shard.mark_clean();
        }
    }

    /// Removes a chunk from the appropriate shard.
    pub fn remove_chunk(&mut self, id: &ChunkID) -> Option<ChunkLocation> {
        let shard_idx = Self::shard_index(id);
        self.dirty = true;
        self.shards[shard_idx].remove_chunk(id)
    }

    /// Compacts all shards by removing unused chunks.
    pub fn compact(&mut self, used_chunks: &std::collections::HashSet<ChunkID>) -> usize {
        let mut total_removed = 0;
        for shard in &mut self.shards {
            total_removed += shard.compact(used_chunks);
        }
        if total_removed > 0 {
            self.dirty = true;
        }
        total_removed
    }

    /// Returns all chunk IDs across all shards.
    pub fn all_chunk_ids(&self) -> std::collections::HashSet<ChunkID> {
        let mut all = std::collections::HashSet::new();
        for shard in &self.shards {
            all.extend(shard.all_chunk_ids());
        }
        all
    }

    /// Returns all pack IDs.
    pub fn all_pack_ids(&self) -> Vec<PackID> {
        self.packs.keys().cloned().collect()
    }

    /// Returns statistics about shard distribution.
    pub fn shard_stats(&self) -> ShardStats {
        let counts: Vec<usize> = self.shards.iter().map(|s| s.chunk_count()).collect();
        let total: usize = counts.iter().sum();
        let non_empty = counts.iter().filter(|&&c| c > 0).count();
        let max = *counts.iter().max().unwrap_or(&0);
        let min = *counts.iter().filter(|&&c| c > 0).min().unwrap_or(&0);

        ShardStats {
            total_chunks: total,
            shard_count: SHARD_COUNT,
            non_empty_shards: non_empty,
            max_shard_size: max,
            min_shard_size: min,
            avg_shard_size: total.checked_div(non_empty).unwrap_or(0),
        }
    }

    /// Saves the sharded index to encrypted files.
    pub async fn save_encrypted<P: AsRef<Path>>(
        &self,
        base_path: P,
        encryptor: &Encryptor,
    ) -> Result<()> {
        let base_path = base_path.as_ref();

        // Ensure directory exists
        fs::create_dir_all(base_path).await?;

        // Save header
        let header = ShardedIndexHeader {
            version: INDEX_VERSION,
            shard_count: SHARD_COUNT as u32,
            total_chunks: self.chunk_count() as u64,
            total_packs: self.pack_count() as u64,
        };

        let header_data = postcard::to_allocvec(&header)
            .map_err(|e| Error::Other(format!("Header serialization failed: {}", e)))?;
        let header_encrypted = encryptor.encrypt(&header_data)?;
        fs::write(base_path.join("header.idx"), &header_encrypted).await?;

        // Save packs file
        let packs_data = postcard::to_allocvec(&self.packs)
            .map_err(|e| Error::Other(format!("Packs serialization failed: {}", e)))?;
        let packs_encrypted = encryptor.encrypt(&packs_data)?;
        fs::write(base_path.join("packs.idx"), &packs_encrypted).await?;

        // Save each non-empty shard
        for (idx, shard) in self.shards.iter().enumerate() {
            if shard.chunk_count() > 0 {
                let shard_path = base_path.join(format!("shard_{:02x}.idx", idx));
                shard.save_encrypted(&shard_path, encryptor).await?;
            }
        }

        Ok(())
    }

    /// Loads the sharded index from encrypted files.
    pub async fn load_encrypted<P: AsRef<Path>>(
        base_path: P,
        encryptor: &Encryptor,
    ) -> Result<Self> {
        let base_path = base_path.as_ref();

        // Load header
        let header_encrypted = fs::read(base_path.join("header.idx")).await?;
        let header_data = encryptor.decrypt(&header_encrypted)?;
        let header: ShardedIndexHeader = postcard::from_bytes(&header_data)
            .map_err(|e| Error::Other(format!("Header deserialization failed: {}", e)))?;

        if header.version > INDEX_VERSION {
            return Err(Error::Other(format!(
                "Index version {} is newer than supported version {}",
                header.version, INDEX_VERSION
            )));
        }

        // Load packs
        let packs_encrypted = fs::read(base_path.join("packs.idx")).await?;
        let packs_data = encryptor.decrypt(&packs_encrypted)?;
        let packs: HashMap<PackID, PackInfo> = postcard::from_bytes(&packs_data)
            .map_err(|e| Error::Other(format!("Packs deserialization failed: {}", e)))?;

        // Load all shards
        let mut shards: Vec<Index> = (0..SHARD_COUNT).map(|_| Index::new()).collect();

        for (idx, shard) in shards.iter_mut().enumerate() {
            let shard_path = base_path.join(format!("shard_{:02x}.idx", idx));
            if shard_path.exists() {
                *shard = Index::load_encrypted(&shard_path, encryptor).await?;
            }
        }

        Ok(Self {
            shards,
            packs,
            dirty: false,
        })
    }

    /// Checks if sharded index exists at path.
    pub async fn exists<P: AsRef<Path>>(base_path: P) -> bool {
        base_path.as_ref().join("header.idx").exists()
    }
}

impl Default for ShardedIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about shard distribution.
#[derive(Debug, Clone)]
pub struct ShardStats {
    pub total_chunks: usize,
    pub shard_count: usize,
    pub non_empty_shards: usize,
    pub max_shard_size: usize,
    pub min_shard_size: usize,
    pub avg_shard_size: usize,
}

/// Determines if sharding should be used based on chunk count.
pub fn should_use_sharding(chunk_count: usize) -> bool {
    chunk_count >= SHARD_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_basic_operations() {
        let mut index = Index::new();
        let chunk_id = ChunkID::from_data(b"test data");
        let location = ChunkLocation {
            pack_id: "pack-123".to_string(),
            offset: 0,
            length: 100,
        };

        assert!(!index.has_chunk(&chunk_id));
        index.add_chunk(chunk_id, location.clone());
        assert!(index.has_chunk(&chunk_id));

        let retrieved = index.get_chunk(&chunk_id).unwrap();
        assert_eq!(retrieved.pack_id, location.pack_id);
    }

    #[test]
    fn test_bloom_filter_no_false_negatives() {
        let mut index = Index::new();
        let mut chunk_ids = Vec::new();

        // Add 1000 chunks
        for i in 0..1000 {
            let chunk_id = ChunkID::from_data(format!("chunk-{}", i).as_bytes());
            chunk_ids.push(chunk_id);
            index.add_chunk(
                chunk_id,
                ChunkLocation {
                    pack_id: "pack".to_string(),
                    offset: i as u64,
                    length: 100,
                },
            );
        }

        // Verify all chunks are found (no false negatives)
        for chunk_id in &chunk_ids {
            assert!(index.has_chunk(chunk_id), "Bloom filter false negative!");
        }
    }

    #[test]
    fn test_index_merge() {
        let mut index1 = Index::new();
        let mut index2 = Index::new();

        let chunk1 = ChunkID::from_data(b"chunk1");
        let chunk2 = ChunkID::from_data(b"chunk2");

        index1.add_chunk(
            chunk1,
            ChunkLocation {
                pack_id: "pack1".to_string(),
                offset: 0,
                length: 100,
            },
        );

        index2.add_chunk(
            chunk2,
            ChunkLocation {
                pack_id: "pack2".to_string(),
                offset: 0,
                length: 200,
            },
        );

        index1.merge(index2);

        assert!(index1.has_chunk(&chunk1));
        assert!(index1.has_chunk(&chunk2));
        assert_eq!(index1.chunk_count(), 2);
    }
}
