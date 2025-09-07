use crate::Result;
use fastcdc::v2020::FastCDC;
use std::io::Read;

pub struct Chunker {
    min_size: u32,
    avg_size: u32,
    max_size: u32,
}

impl Chunker {
    pub fn new(avg_size: u32) -> Self {
        Self {
            min_size: avg_size / 4,
            avg_size,
            max_size: avg_size * 4,
        }
    }
    
    pub fn default() -> Self {
        Self::new(4 * 1024 * 1024)
    }
    
    pub fn chunk_data(&self, data: &[u8]) -> Vec<Chunk> {
        let chunker = FastCDC::new(data, self.min_size, self.avg_size, self.max_size);
        chunker
            .map(|chunk| Chunk {
                offset: chunk.offset,
                length: chunk.length,
                data: data[chunk.offset..chunk.offset + chunk.length].to_vec(),
            })
            .collect()
    }
    
    pub fn chunk_reader<R: Read>(&self, mut reader: R) -> Result<Vec<Chunk>> {
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer)?;
        Ok(self.chunk_data(&buffer))
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub offset: usize,
    pub length: usize,
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn id(&self) -> crate::ChunkID {
        crate::ChunkID::from(blake3::hash(&self.data))
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_chunking() {
        let chunker = Chunker::new(1024);
        let data = vec![0u8; 10000];
        let chunks = chunker.chunk_data(&data);
        
        assert!(!chunks.is_empty());
        
        let total_size: usize = chunks.iter().map(|c| c.length).sum();
        assert_eq!(total_size, data.len());
    }
}