use crate::crypto::Encryptor;
use crate::types::{ChunkID, PackID};
use crate::{Error, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Pack file format version for schema evolution
const PACK_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackHeader {
    /// Format version
    #[serde(default = "default_version")]
    pub version: u32,
    pub pack_id: PackID,
    pub chunk_count: u32,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// BLAKE3 hash of the unencrypted data section (for integrity verification)
    #[serde(default)]
    pub data_checksum: Option<String>,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackFile {
    pub header: PackHeader,
    pub chunks: HashMap<ChunkID, PackedChunk>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedChunk {
    pub id: ChunkID,
    pub offset: u64,
    pub length: u32,
    pub uncompressed_length: u32,
}

impl PackFile {
    pub fn new(pack_id: PackID) -> Self {
        Self {
            header: PackHeader {
                version: PACK_VERSION,
                pack_id,
                chunk_count: 0,
                uncompressed_size: 0,
                compressed_size: 0,
                created_at: chrono::Utc::now(),
                data_checksum: None,
            },
            chunks: HashMap::new(),
            data: Vec::new(),
        }
    }

    pub fn add_chunk(&mut self, id: ChunkID, data: &[u8]) -> Result<()> {
        // Compress the chunk data
        let compressed = self.compress_data(data)?;

        let offset = self.data.len() as u64;
        let chunk = PackedChunk {
            id,
            offset,
            length: compressed.len() as u32,
            uncompressed_length: data.len() as u32,
        };

        // Append compressed data to pack
        self.data.extend_from_slice(&compressed);

        self.chunks.insert(id, chunk);
        self.header.chunk_count += 1;
        self.header.uncompressed_size += data.len() as u64;
        self.header.compressed_size += compressed.len() as u64;

        // Invalidate checksum (will be recomputed on write)
        self.header.data_checksum = None;

        Ok(())
    }

    pub fn get_chunk(&self, id: &ChunkID) -> Result<Bytes> {
        let chunk = self
            .chunks
            .get(id)
            .ok_or_else(|| Error::Other(format!("Chunk {:?} not found in pack", id)))?;

        let start = chunk.offset as usize;
        let end = start + chunk.length as usize;

        if end > self.data.len() {
            return Err(Error::Other(
                "Pack data corruption: chunk extends beyond pack data".to_string(),
            ));
        }

        let compressed_data = &self.data[start..end];
        let decompressed = self.decompress_data(compressed_data)?;

        Ok(Bytes::from(decompressed))
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn is_full(&self, max_size: u64) -> bool {
        self.data.len() as u64 >= max_size
    }

    pub fn chunk_ids(&self) -> Vec<ChunkID> {
        self.chunks.keys().cloned().collect()
    }

    /// Computes and sets the data checksum.
    fn compute_checksum(&mut self) {
        let hash = blake3::hash(&self.data);
        self.header.data_checksum = Some(hash.to_hex().to_string());
    }

    /// Verifies the data section against the stored checksum.
    pub fn verify_checksum(&self) -> Result<bool> {
        match &self.header.data_checksum {
            Some(stored) => {
                let computed = blake3::hash(&self.data).to_hex().to_string();
                Ok(computed == *stored)
            }
            None => {
                // No checksum stored (old format pack), can't verify
                Ok(true)
            }
        }
    }

    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(data)
            .map_err(|e| Error::Other(e.to_string()))?;
        encoder.finish().map_err(|e| Error::Other(e.to_string()))
    }

    fn decompress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = flate2::read::ZlibDecoder::new(data);
        let mut result = Vec::new();
        decoder
            .read_to_end(&mut result)
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(result)
    }

    pub async fn write_to<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        encryptor: &Encryptor,
    ) -> Result<()> {
        let bytes = self.to_encrypted_bytes(encryptor)?;
        writer
            .write_all(&bytes)
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(())
    }

    pub fn to_encrypted_bytes(&self, encryptor: &Encryptor) -> Result<Vec<u8>> {
        // Compute checksum before writing
        let mut pack_to_write = self.clone();
        pack_to_write.compute_checksum();

        // Serialize header and chunk index
        let header_data = postcard::to_allocvec(&pack_to_write.header)
            .map_err(|e| Error::Other(e.to_string()))?;
        let chunks_data = postcard::to_allocvec(&pack_to_write.chunks)
            .map_err(|e| Error::Other(e.to_string()))?;

        // Encrypt header and chunk index
        let encrypted_header = encryptor.encrypt(&header_data)?;
        let encrypted_chunks = encryptor.encrypt(&chunks_data)?;
        let encrypted_data = encryptor.encrypt(&pack_to_write.data)?;

        let mut bytes = Vec::with_capacity(
            8 + encrypted_header.len() + encrypted_chunks.len() + encrypted_data.len(),
        );
        bytes.extend_from_slice(&(encrypted_header.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&encrypted_header);
        bytes.extend_from_slice(&(encrypted_chunks.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&encrypted_chunks);
        bytes.extend_from_slice(&encrypted_data);
        Ok(bytes)
    }

    pub async fn read_from<R: AsyncRead + Unpin>(
        reader: &mut R,
        encryptor: &Encryptor,
    ) -> Result<Self> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| Error::Other(e.to_string()))?;
        Self::from_encrypted_bytes(&bytes, encryptor)
    }

    pub fn from_encrypted_bytes(bytes: &[u8], encryptor: &Encryptor) -> Result<Self> {
        let mut cursor = std::io::Cursor::new(bytes);

        // Read header
        let mut u32_buf = [0u8; 4];
        std::io::Read::read_exact(&mut cursor, &mut u32_buf)
            .map_err(|e| Error::Other(e.to_string()))?;
        let header_len = u32::from_le_bytes(u32_buf);
        let mut header_encrypted = vec![0u8; header_len as usize];
        std::io::Read::read_exact(&mut cursor, &mut header_encrypted)
            .map_err(|e| Error::Other(e.to_string()))?;
        let header_data = encryptor.decrypt(&header_encrypted)?;
        let header: PackHeader =
            postcard::from_bytes(&header_data).map_err(|e| Error::Other(e.to_string()))?;

        // Read chunk index
        std::io::Read::read_exact(&mut cursor, &mut u32_buf)
            .map_err(|e| Error::Other(e.to_string()))?;
        let chunks_len = u32::from_le_bytes(u32_buf);
        let mut chunks_encrypted = vec![0u8; chunks_len as usize];
        std::io::Read::read_exact(&mut cursor, &mut chunks_encrypted)
            .map_err(|e| Error::Other(e.to_string()))?;
        let chunks_data = encryptor.decrypt(&chunks_encrypted)?;
        let chunks: HashMap<ChunkID, PackedChunk> =
            postcard::from_bytes(&chunks_data).map_err(|e| Error::Other(e.to_string()))?;

        // Read remaining data as chunk data
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut cursor, &mut data)
            .map_err(|e| Error::Other(e.to_string()))?;
        let decrypted_data = encryptor.decrypt(&data)?;

        let pack = PackFile {
            header,
            chunks,
            data: decrypted_data,
        };

        // Verify checksum if present
        if !pack.verify_checksum()? {
            return Err(Error::CorruptedPack {
                id: pack.header.pack_id.clone(),
            });
        }

        Ok(pack)
    }
}

#[derive(Debug)]
pub struct PackManager {
    current_pack: Option<PackFile>,
    max_pack_size: u64,
    pack_counter: u64,
}

impl PackManager {
    pub fn new(max_pack_size: u64) -> Self {
        Self {
            current_pack: None,
            max_pack_size,
            pack_counter: 0,
        }
    }

    pub fn add_chunk(&mut self, chunk_id: ChunkID, data: &[u8]) -> Result<Option<PackFile>> {
        // Check if we need a new pack
        if self.current_pack.is_none()
            || self
                .current_pack
                .as_ref()
                .unwrap()
                .is_full(self.max_pack_size)
        {
            let finished_pack = self.current_pack.take();
            self.start_new_pack()?;

            // Add the chunk to the new pack
            if let Some(pack) = self.current_pack.as_mut() {
                pack.add_chunk(chunk_id, data)?;
            }

            return Ok(finished_pack);
        }

        // Add to current pack
        if let Some(pack) = self.current_pack.as_mut() {
            pack.add_chunk(chunk_id, data)?;
        }

        Ok(None)
    }

    pub fn finish_current_pack(&mut self) -> Option<PackFile> {
        self.current_pack.take()
    }

    fn start_new_pack(&mut self) -> Result<()> {
        // Use UUID for globally unique pack IDs to avoid collisions across backups
        let pack_id = uuid::Uuid::new_v4().to_string();
        self.pack_counter += 1;
        self.current_pack = Some(PackFile::new(pack_id));
        Ok(())
    }
}

/// Statistics from a repack operation.
#[derive(Debug, Default)]
pub struct RepackStats {
    pub packs_read: usize,
    pub packs_written: usize,
    pub chunks_copied: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Repacker for consolidating and cleaning up packs.
pub struct Repacker {
    #[allow(dead_code)]
    max_pack_size: u64,
    min_pack_size: u64,
}

impl Repacker {
    pub fn new(max_pack_size: u64) -> Self {
        Self {
            max_pack_size,
            min_pack_size: max_pack_size / 4, // Packs smaller than 25% of max are candidates for repack
        }
    }

    /// Creates a new pack containing only the specified chunks from the source pack.
    pub fn extract_chunks(
        &self,
        source_pack: &PackFile,
        chunk_ids: &[ChunkID],
    ) -> Result<Option<PackFile>> {
        if chunk_ids.is_empty() {
            return Ok(None);
        }

        let mut new_pack = PackFile::new(uuid::Uuid::new_v4().to_string());

        for chunk_id in chunk_ids {
            if let Some(chunk_entry) = source_pack.chunks.get(chunk_id) {
                // Get the raw compressed data from source pack
                let start = chunk_entry.offset as usize;
                let end = start + chunk_entry.length as usize;
                let compressed_data = &source_pack.data[start..end];

                // Decompress to get original data
                let decompressed = source_pack.decompress_data(compressed_data)?;

                // Add to new pack (will be recompressed)
                new_pack.add_chunk(*chunk_id, &decompressed)?;
            }
        }

        if new_pack.chunks.is_empty() {
            Ok(None)
        } else {
            Ok(Some(new_pack))
        }
    }

    /// Identifies packs that are candidates for repacking (too small or too fragmented).
    pub fn find_repack_candidates(&self, pack_infos: &[(String, u64)]) -> Vec<String> {
        pack_infos
            .iter()
            .filter(|(_, size)| *size < self.min_pack_size)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_checksum() {
        let mut pack = PackFile::new("test-pack".to_string());
        pack.add_chunk(ChunkID::from_data(b"chunk1"), b"hello world")
            .unwrap();
        pack.add_chunk(ChunkID::from_data(b"chunk2"), b"goodbye world")
            .unwrap();

        // Compute checksum
        pack.compute_checksum();
        assert!(pack.header.data_checksum.is_some());

        // Verify checksum
        assert!(pack.verify_checksum().unwrap());

        // Corrupt data and verify fails
        if !pack.data.is_empty() {
            pack.data[0] ^= 0xFF;
        }
        assert!(!pack.verify_checksum().unwrap());
    }

    #[test]
    fn test_repacker_extract_chunks() {
        let mut source = PackFile::new("source".to_string());
        let chunk1 = ChunkID::from_data(b"chunk1");
        let chunk2 = ChunkID::from_data(b"chunk2");
        let chunk3 = ChunkID::from_data(b"chunk3");

        source.add_chunk(chunk1, b"data1").unwrap();
        source.add_chunk(chunk2, b"data2").unwrap();
        source.add_chunk(chunk3, b"data3").unwrap();

        let repacker = Repacker::new(64 * 1024 * 1024);

        // Extract only chunk1 and chunk3
        let new_pack = repacker
            .extract_chunks(&source, &[chunk1, chunk3])
            .unwrap()
            .unwrap();

        assert_eq!(new_pack.chunks.len(), 2);
        assert!(new_pack.chunks.contains_key(&chunk1));
        assert!(new_pack.chunks.contains_key(&chunk3));
        assert!(!new_pack.chunks.contains_key(&chunk2));
    }
}
