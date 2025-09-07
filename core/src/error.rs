use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Encryption error: {0}")]
    Encryption(String),
    
    #[error("Repository not found at {path}")]
    RepositoryNotFound { path: String },
    
    #[error("Repository already exists at {path}")]
    RepositoryExists { path: String },
    
    #[error("Invalid repository format version: {version}")]
    InvalidFormatVersion { version: u32 },
    
    #[error("Pack file corrupted: {id}")]
    CorruptedPack { id: String },
    
    #[error("Snapshot not found: {id}")]
    SnapshotNotFound { id: String },
    
    #[error("Invalid password")]
    InvalidPassword,
    
    #[error("Backend error: {0}")]
    Backend(String),
    
    #[error("Chunk not found: {id}")]
    ChunkNotFound { id: String },
    
    #[error("Lock conflict: {0}")]
    LockConflict(String),
    
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;