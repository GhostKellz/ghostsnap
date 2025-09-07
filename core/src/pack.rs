use crate::crypto::Encryptor;
use crate::types::{ChunkID, PackID};
use crate::{Result, Error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackHeader {
    pub pack_id: PackID,
    pub chunk_count: u32,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
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
                pack_id,
                chunk_count: 0,
                uncompressed_size: 0,
                compressed_size: 0,
                created_at: chrono::Utc::now(),
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
            id: id.clone(),
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
        
        Ok(())
    }
    
    pub fn get_chunk(&self, id: &ChunkID) -> Result<Bytes> {
        let chunk = self.chunks.get(id)
            .ok_or_else(|| Error::Other(format!("Chunk {:?} not found in pack", id)))?;
        
        let start = chunk.offset as usize;
        let end = start + chunk.length as usize;
        
        if end > self.data.len() {
            return Err(Error::Other("Pack data corruption: chunk extends beyond pack data".to_string()));
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

    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).map_err(|e| Error::Other(e.to_string()))?;
        encoder.finish().map_err(|e| Error::Other(e.to_string()))
    }

    fn decompress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = flate2::read::ZlibDecoder::new(data);
        let mut result = Vec::new();
        decoder.read_to_end(&mut result).map_err(|e| Error::Other(e.to_string()))?;
        Ok(result)
    }

    pub async fn write_to<W: AsyncWrite + Unpin>(&self, writer: &mut W, encryptor: &Encryptor) -> Result<()> {
        // Serialize header and chunk index
        let header_data = bincode::serialize(&self.header).map_err(|e| Error::Other(e.to_string()))?;
        let chunks_data = bincode::serialize(&self.chunks).map_err(|e| Error::Other(e.to_string()))?;
        
        // Write header length and encrypted header
        writer.write_u32_le(header_data.len() as u32).await.map_err(|e| Error::Other(e.to_string()))?;
        let encrypted_header = encryptor.encrypt(&header_data)?;
        writer.write_all(&encrypted_header).await.map_err(|e| Error::Other(e.to_string()))?;
        
        // Write chunk index length and encrypted index
        writer.write_u32_le(chunks_data.len() as u32).await.map_err(|e| Error::Other(e.to_string()))?;
        let encrypted_chunks = encryptor.encrypt(&chunks_data)?;
        writer.write_all(&encrypted_chunks).await.map_err(|e| Error::Other(e.to_string()))?;
        
        // Write encrypted chunk data
        let encrypted_data = encryptor.encrypt(&self.data)?;
        writer.write_all(&encrypted_data).await.map_err(|e| Error::Other(e.to_string()))?;
        
        Ok(())
    }

    pub async fn read_from<R: AsyncRead + Unpin>(reader: &mut R, encryptor: &Encryptor) -> Result<Self> {
        // Read header
        let header_len = reader.read_u32_le().await.map_err(|e| Error::Other(e.to_string()))?;
        let mut header_encrypted = vec![0u8; header_len as usize];
        reader.read_exact(&mut header_encrypted).await.map_err(|e| Error::Other(e.to_string()))?;
        let header_data = encryptor.decrypt(&header_encrypted)?;
        let header: PackHeader = bincode::deserialize(&header_data).map_err(|e| Error::Other(e.to_string()))?;
        
        // Read chunk index
        let chunks_len = reader.read_u32_le().await.map_err(|e| Error::Other(e.to_string()))?;
        let mut chunks_encrypted = vec![0u8; chunks_len as usize];
        reader.read_exact(&mut chunks_encrypted).await.map_err(|e| Error::Other(e.to_string()))?;
        let chunks_data = encryptor.decrypt(&chunks_encrypted)?;
        let chunks: HashMap<ChunkID, PackedChunk> = bincode::deserialize(&chunks_data).map_err(|e| Error::Other(e.to_string()))?;
        
        // Read remaining data as chunk data
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await.map_err(|e| Error::Other(e.to_string()))?;
        let decrypted_data = encryptor.decrypt(&data)?;
        
        Ok(PackFile {
            header,
            chunks,
            data: decrypted_data,
        })
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
        if self.current_pack.is_none() || 
           self.current_pack.as_ref().unwrap().is_full(self.max_pack_size) {
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
        let pack_id = format!("pack-{:08x}", self.pack_counter);
        self.pack_counter += 1;
        self.current_pack = Some(PackFile::new(pack_id));
        Ok(())
    }
}