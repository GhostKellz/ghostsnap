use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::PathBuf;
use std::str::FromStr;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkID(blake3::Hash);

impl ChunkID {
    pub fn new(hash: blake3::Hash) -> Self {
        Self(hash)
    }

    pub fn from_data(data: &[u8]) -> Self {
        Self(blake3::hash(data))
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
    
    pub fn to_hex(&self) -> String {
        self.0.to_hex().to_string()
    }

    pub fn short_string(&self) -> String {
        self.to_hex().chars().take(8).collect()
    }
}

impl From<blake3::Hash> for ChunkID {
    fn from(hash: blake3::Hash) -> Self {
        Self(hash)
    }
}

impl FromStr for ChunkID {
    type Err = hex::FromHexError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self(blake3::Hash::from(array)))
    }
}

impl Serialize for ChunkID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ChunkID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ChunkID::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for ChunkID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}
pub type SnapshotID = String;
pub type PackID = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub version: u32,
    pub id: String,
    pub chunker_polynomial: u64,
    pub kdf_params: KdfParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfParams {
    pub algorithm: String,
    pub iterations: u32,
    pub memory: u32,
    pub parallelism: u32,
    pub salt: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub id: ChunkID,
    pub pack_id: PackID,
    pub offset: u64,
    pub length: u32,
    pub uncompressed_length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRef {
    pub id: ChunkID,
    pub offset: u64,
    pub length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub mtime: i64,
    pub ctime: i64,
    pub chunks: Vec<ChunkID>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub name: String,
    pub node_type: NodeType,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub mtime: i64,
    pub subtree_id: Option<ChunkID>,
    pub chunks: Vec<ChunkRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    File,
    Directory,
    Symlink,
}

impl TreeNode {
    pub fn is_file(&self) -> bool {
        matches!(self.node_type, NodeType::File)
    }

    pub fn is_dir(&self) -> bool {
        matches!(self.node_type, NodeType::Directory)
    }

    pub fn is_symlink(&self) -> bool {
        matches!(self.node_type, NodeType::Symlink)
    }
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            version: 1,
            id: uuid::Uuid::new_v4().to_string(),
            chunker_polynomial: 0x3DA3358B4DC173,
            kdf_params: KdfParams::default(),
        }
    }
}

impl Default for KdfParams {
    fn default() -> Self {
        use rand::RngCore;
        let mut salt = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);
        
        Self {
            algorithm: "argon2id".to_string(),
            iterations: 1,
            memory: 65536,
            parallelism: 4,
            salt,
        }
    }
}

use uuid;