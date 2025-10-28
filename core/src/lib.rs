//! Core functionality for Ghostsnap backup system.
//!
//! This crate provides the fundamental building blocks for a secure, deduplicating
//! backup system with support for encryption, content-defined chunking, and efficient
//! pack-based storage.
//!
//! # Features
//!
//! - **Content-Defined Chunking**: Uses FastCDC for efficient deduplication
//! - **Strong Encryption**: ChaCha20-Poly1305 authenticated encryption
//! - **Pack-Based Storage**: Efficient aggregation of small chunks
//! - **Repository Management**: File-based or backend-agnostic storage
//!
//! # Example
//!
//! ```no_run
//! use ghostsnap_core::Repository;
//!
//! #[tokio::main]
//! async fn main() -> ghostsnap_core::Result<()> {
//!     // Initialize a new repository
//!     let repo = Repository::init("./my-backup", "secure-password").await?;
//!
//!     // Repository is now ready for backups
//!     Ok(())
//! }
//! ```

pub mod chunker;
pub mod crypto;
pub mod error;
pub mod index;
pub mod pack;
pub mod repository;
pub mod snapshot;
pub mod types;

pub use error::{Error, Result};
pub use repository::Repository;
pub use snapshot::Snapshot;
pub use types::*;