pub mod backend;
pub mod local;
pub mod s3;
pub mod azure_simple;
pub mod minio;

pub use backend::{Backend, BackendType, ObjectInfo};
pub use local::LocalBackend;
pub use s3::S3Backend;
pub use azure_simple::AzureSimpleBackend;
pub use minio::{MinIOBackend, MinIOConfig, BucketMetrics};