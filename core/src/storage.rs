use crate::{Result, S3RepoSse};
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::ServerSideEncryption;
use bytes::Bytes;
use chrono::Utc;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum RepositoryLocation {
    Local(PathBuf),
    S3(S3Location),
    Azure(AzureLocation),
    Rclone(RcloneLocation),
    Sftp(SftpLocation),
}

impl RepositoryLocation {
    pub fn display(&self) -> String {
        match self {
            Self::Local(path) => path.display().to_string(),
            Self::S3(location) => location.display(),
            Self::Azure(location) => location.display(),
            Self::Rclone(location) => location.display(),
            Self::Sftp(location) => location.display(),
        }
    }

    pub fn parse(input: &str) -> crate::Result<Self> {
        // S3 URIs
        if let Some(rest) = input.strip_prefix("s3://") {
            return parse_s3_location(rest);
        }
        if let Some(rest) = input.strip_prefix("s3:") {
            return parse_s3_location(rest);
        }

        // Azure URIs
        if let Some(rest) = input.strip_prefix("azure://") {
            return parse_azure_location(rest);
        }
        if let Some(rest) = input.strip_prefix("azure:") {
            return parse_azure_location(rest);
        }

        // Rclone URIs
        if let Some(rest) = input.strip_prefix("rclone://") {
            return parse_rclone_location(rest);
        }
        if let Some(rest) = input.strip_prefix("rclone:") {
            return parse_rclone_location(rest);
        }

        // Backblaze B2 and MinIO are S3-compatible: parse them into an S3
        // location with the appropriate endpoint resolved from the environment.
        if let Some(rest) = input.strip_prefix("b2://") {
            return parse_b2_location(rest);
        }
        if let Some(rest) = input.strip_prefix("b2:") {
            return parse_b2_location(rest);
        }
        if let Some(rest) = input.strip_prefix("minio://") {
            return parse_minio_location(rest);
        }
        if let Some(rest) = input.strip_prefix("minio:") {
            return parse_minio_location(rest);
        }

        // SFTP URIs
        if let Some(rest) = input.strip_prefix("sftp://") {
            return parse_sftp_location(rest);
        }
        if let Some(rest) = input.strip_prefix("sftp:") {
            return parse_sftp_location(rest);
        }

        Ok(Self::Local(PathBuf::from(input)))
    }
}

// =============================================================================
// S3 Location
// =============================================================================

#[derive(Debug, Clone)]
pub struct S3Location {
    pub bucket: String,
    pub prefix: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub sse: Option<S3RepoSse>,
}

impl S3Location {
    pub fn new(bucket: String, prefix: String) -> Self {
        Self {
            bucket,
            prefix,
            endpoint: None,
            region: None,
            sse: None,
        }
    }

    pub fn display(&self) -> String {
        if self.prefix.is_empty() {
            format!("s3:{}", self.bucket)
        } else {
            format!("s3:{}/{}", self.bucket, self.prefix)
        }
    }

    fn key(&self, path: &str) -> String {
        if self.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.prefix.trim_end_matches('/'), path)
        }
    }

    /// Applies environment variable overrides for endpoint and region.
    ///
    /// Checks these environment variables (in order of priority):
    /// - `AWS_ENDPOINT_URL` - S3-compatible endpoint URL (Wasabi, Backblaze B2, MinIO)
    /// - `AWS_REGION` or `AWS_DEFAULT_REGION` - AWS region
    ///
    /// Only applies overrides if the field is not already set.
    pub fn with_env_overrides(mut self) -> Self {
        if self.endpoint.is_none()
            && let Ok(endpoint) = std::env::var("AWS_ENDPOINT_URL")
        {
            self.endpoint = Some(endpoint);
        }
        if self.region.is_none() {
            if let Ok(region) = std::env::var("AWS_REGION") {
                self.region = Some(region);
            } else if let Ok(region) = std::env::var("AWS_DEFAULT_REGION") {
                self.region = Some(region);
            }
        }
        self
    }
}

fn parse_s3_location(input: &str) -> crate::Result<RepositoryLocation> {
    let trimmed = input.trim_matches('/');
    if trimmed.is_empty() {
        return Err(crate::Error::Other(
            "S3 repository URI must include a bucket name".to_string(),
        ));
    }

    let (bucket, prefix) = match trimmed.split_once('/') {
        Some((bucket, prefix)) => {
            if bucket.is_empty() {
                return Err(crate::Error::Other(
                    "S3 repository URI must include a bucket name".to_string(),
                ));
            }
            (bucket.to_string(), prefix.to_string())
        }
        None => (trimmed.to_string(), String::new()),
    };

    Ok(RepositoryLocation::S3(S3Location::new(bucket, prefix)))
}

/// Split a `bucket/prefix` string shared by the S3-compatible schemes.
fn split_bucket_prefix(input: &str, scheme: &str) -> crate::Result<(String, String)> {
    let trimmed = input.trim_matches('/');
    if trimmed.is_empty() {
        return Err(crate::Error::Other(format!(
            "{} repository URI must include a bucket name",
            scheme
        )));
    }
    match trimmed.split_once('/') {
        Some((bucket, prefix)) if !bucket.is_empty() => {
            Ok((bucket.to_string(), prefix.to_string()))
        }
        Some(_) => Err(crate::Error::Other(format!(
            "{} repository URI must include a bucket name",
            scheme
        ))),
        None => Ok((trimmed.to_string(), String::new())),
    }
}

/// Parse a `b2:bucket/prefix` URI into an S3-compatible location.
///
/// Backblaze B2 exposes an S3-compatible API. The endpoint is region-specific
/// (e.g. `https://s3.us-west-002.backblazeb2.com`) and is resolved from
/// `B2_S3_ENDPOINT` or `AWS_ENDPOINT_URL`. Credentials come from the standard
/// AWS credential chain (`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`, set to
/// the B2 keyID / applicationKey). When no endpoint is set the value is left
/// empty so a persisted repository transport can supply it on reopen.
fn parse_b2_location(input: &str) -> crate::Result<RepositoryLocation> {
    let (bucket, prefix) = split_bucket_prefix(input, "B2")?;

    let endpoint = std::env::var("B2_S3_ENDPOINT")
        .or_else(|_| std::env::var("AWS_ENDPOINT_URL"))
        .ok();
    // B2's S3 region is embedded in the endpoint host (s3.<region>.backblazeb2.com).
    let region = std::env::var("AWS_REGION")
        .ok()
        .or_else(|| endpoint.as_deref().and_then(b2_region_from_endpoint));

    let mut location = S3Location::new(bucket, prefix);
    location.endpoint = endpoint;
    location.region = region;
    Ok(RepositoryLocation::S3(location))
}

/// Extract the B2 region label from an S3 endpoint host such as
/// `https://s3.us-west-002.backblazeb2.com` → `us-west-002`.
fn b2_region_from_endpoint(endpoint: &str) -> Option<String> {
    let host = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = host.split('/').next().unwrap_or(host);
    let mut labels = host.split('.');
    if labels.next()? != "s3" {
        return None;
    }
    let region = labels.next()?;
    if region.is_empty() {
        None
    } else {
        Some(region.to_string())
    }
}

/// Parse a `minio:bucket/prefix` URI into an S3-compatible location.
///
/// MinIO is self-hosted, so the endpoint is resolved from `MINIO_ENDPOINT` or
/// `AWS_ENDPOINT_URL`. The region defaults to `us-east-1` (MinIO ignores it but
/// the AWS SDK requires a value). Credentials come from the standard AWS
/// credential chain.
fn parse_minio_location(input: &str) -> crate::Result<RepositoryLocation> {
    let (bucket, prefix) = split_bucket_prefix(input, "MinIO")?;

    let endpoint = std::env::var("MINIO_ENDPOINT")
        .or_else(|_| std::env::var("AWS_ENDPOINT_URL"))
        .ok();
    let region = std::env::var("AWS_REGION")
        .ok()
        .or_else(|| Some("us-east-1".to_string()));

    let mut location = S3Location::new(bucket, prefix);
    location.endpoint = endpoint;
    location.region = region;
    Ok(RepositoryLocation::S3(location))
}

// =============================================================================
// Azure Location
// =============================================================================

#[derive(Debug, Clone)]
pub struct AzureLocation {
    pub account_name: String,
    pub container: String,
    pub prefix: String,
}

impl AzureLocation {
    pub fn new(account_name: String, container: String, prefix: String) -> Self {
        Self {
            account_name,
            container,
            prefix,
        }
    }

    pub fn display(&self) -> String {
        if self.prefix.is_empty() {
            format!("azure:{}/{}", self.account_name, self.container)
        } else {
            format!("azure:{}/{}/{}", self.account_name, self.container, self.prefix)
        }
    }

    fn key(&self, path: &str) -> String {
        if self.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.prefix.trim_end_matches('/'), path)
        }
    }

    /// Applies environment variable overrides for account name.
    pub fn with_env_overrides(mut self) -> Self {
        if self.account_name.is_empty()
            && let Ok(account) = std::env::var("AZURE_STORAGE_ACCOUNT")
        {
            self.account_name = account;
        }
        self
    }
}

/// Parse azure:account/container/prefix URI
fn parse_azure_location(input: &str) -> crate::Result<RepositoryLocation> {
    let trimmed = input.trim_matches('/');
    if trimmed.is_empty() {
        return Err(crate::Error::Other(
            "Azure repository URI must include account name and container: azure:account/container[/prefix]".to_string(),
        ));
    }

    let parts: Vec<&str> = trimmed.splitn(3, '/').collect();

    match parts.len() {
        1 => Err(crate::Error::Other(
            "Azure repository URI must include container: azure:account/container[/prefix]".to_string(),
        )),
        2 => Ok(RepositoryLocation::Azure(AzureLocation::new(
            parts[0].to_string(),
            parts[1].to_string(),
            String::new(),
        ))),
        3 => Ok(RepositoryLocation::Azure(AzureLocation::new(
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        ))),
        _ => unreachable!(),
    }
}

// =============================================================================
// Rclone Location
// =============================================================================

#[derive(Debug, Clone)]
pub struct RcloneLocation {
    /// The rclone remote name (as configured in rclone config)
    pub remote: String,
    /// Path within the remote
    pub path: String,
}

impl RcloneLocation {
    pub fn new(remote: String, path: String) -> Self {
        Self { remote, path }
    }

    pub fn display(&self) -> String {
        if self.path.is_empty() {
            format!("rclone:{}", self.remote)
        } else {
            format!("rclone:{}/{}", self.remote, self.path)
        }
    }

    fn key(&self, subpath: &str) -> String {
        if self.path.is_empty() {
            subpath.to_string()
        } else {
            format!("{}/{}", self.path.trim_end_matches('/'), subpath)
        }
    }
}

/// Parse rclone:remote/path URI
fn parse_rclone_location(input: &str) -> crate::Result<RepositoryLocation> {
    let trimmed = input.trim_matches('/');
    if trimmed.is_empty() {
        return Err(crate::Error::Other(
            "Rclone repository URI must include a remote name: rclone:remote[/path]".to_string(),
        ));
    }

    let (remote, path) = match trimmed.split_once('/') {
        Some((remote, path)) => {
            if remote.is_empty() {
                return Err(crate::Error::Other(
                    "Rclone repository URI must include a remote name".to_string(),
                ));
            }
            (remote.to_string(), path.to_string())
        }
        None => (trimmed.to_string(), String::new()),
    };

    Ok(RepositoryLocation::Rclone(RcloneLocation::new(remote, path)))
}

// =============================================================================
// SFTP Location
// =============================================================================

#[derive(Debug, Clone)]
pub struct SftpLocation {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Path on the remote host (relative to the login directory unless absolute).
    pub path: String,
}

impl SftpLocation {
    pub fn new(host: String, port: u16, user: String, path: String) -> Self {
        Self {
            host,
            port,
            user,
            path,
        }
    }

    pub fn display(&self) -> String {
        let user = if self.user.is_empty() {
            String::new()
        } else {
            format!("{}@", self.user)
        };
        let port = if self.port == 22 {
            String::new()
        } else {
            format!(":{}", self.port)
        };
        if self.path.is_empty() {
            format!("sftp:{}{}{}", user, self.host, port)
        } else {
            format!("sftp:{}{}{}/{}", user, self.host, port, self.path)
        }
    }

    fn key(&self, path: &str) -> String {
        if self.path.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.path.trim_end_matches('/'), path)
        }
    }

    /// Applies environment variable overrides for the SSH user.
    pub fn with_env_overrides(mut self) -> Self {
        if self.user.is_empty()
            && let Ok(user) = std::env::var("SFTP_USER").or_else(|_| std::env::var("USER"))
        {
            self.user = user;
        }
        self
    }
}

/// Parse an `sftp:[user@]host[:port][/path]` URI.
fn parse_sftp_location(input: &str) -> crate::Result<RepositoryLocation> {
    let trimmed = input.trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(crate::Error::Other(
            "SFTP repository URI must include a host: sftp:[user@]host[:port][/path]".to_string(),
        ));
    }

    // Split off the path at the first '/' after the authority.
    let (authority, path) = match trimmed.split_once('/') {
        Some((authority, path)) => (authority, path.to_string()),
        None => (trimmed, String::new()),
    };

    // Split optional user.
    let (user, host_port) = match authority.split_once('@') {
        Some((user, host_port)) => (user.to_string(), host_port),
        None => (String::new(), authority),
    };

    if host_port.is_empty() {
        return Err(crate::Error::Other(
            "SFTP repository URI must include a host: sftp:[user@]host[:port][/path]".to_string(),
        ));
    }

    // Split optional port.
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port_str)) => {
            let port = port_str.parse::<u16>().map_err(|_| {
                crate::Error::Other(format!("Invalid SFTP port '{}'", port_str))
            })?;
            (host.to_string(), port)
        }
        None => (host_port.to_string(), 22),
    };

    Ok(RepositoryLocation::Sftp(SftpLocation::new(
        host, port, user, path,
    )))
}

// =============================================================================
// Object Metadata
// =============================================================================

#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    pub size: u64,
    pub modified_at: chrono::DateTime<Utc>,
}

// =============================================================================
// Repository Storage Trait
// =============================================================================

#[async_trait]
pub trait RepositoryStorage: Send + Sync {
    fn location(&self) -> &RepositoryLocation;
    async fn init(&self) -> Result<()>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn read(&self, path: &str) -> Result<Bytes>;
    async fn write(&self, path: &str, data: Bytes) -> Result<()>;
    async fn delete(&self, path: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn metadata(&self, path: &str) -> Result<ObjectMetadata>;
}

pub fn local_storage<P: AsRef<Path>>(path: P) -> Box<dyn RepositoryStorage> {
    Box::new(LocalRepositoryStorage::new(path.as_ref().to_path_buf()))
}

pub async fn s3_storage(location: S3Location) -> Result<Box<dyn RepositoryStorage>> {
    Ok(Box::new(S3RepositoryStorage::new(location).await?))
}

pub async fn azure_storage(location: AzureLocation) -> Result<Box<dyn RepositoryStorage>> {
    Ok(Box::new(AzureRepositoryStorage::new(location).await?))
}

pub fn rclone_storage(location: RcloneLocation) -> Box<dyn RepositoryStorage> {
    Box::new(RcloneRepositoryStorage::new(location))
}

pub async fn sftp_storage(location: SftpLocation) -> Result<Box<dyn RepositoryStorage>> {
    Ok(Box::new(SftpRepositoryStorage::new(location).await?))
}

pub async fn storage_for_location(
    location: &RepositoryLocation,
) -> Result<Box<dyn RepositoryStorage>> {
    match location {
        RepositoryLocation::Local(path) => Ok(local_storage(path)),
        RepositoryLocation::S3(location) => {
            // Apply environment variable overrides for bootstrap.
            // This allows S3-compatible providers (Wasabi, Backblaze B2, MinIO)
            // to set AWS_ENDPOINT_URL before opening an existing repository.
            let location = location.clone().with_env_overrides();
            s3_storage(location).await
        }
        RepositoryLocation::Azure(location) => {
            let location = location.clone().with_env_overrides();
            azure_storage(location).await
        }
        RepositoryLocation::Rclone(location) => Ok(rclone_storage(location.clone())),
        RepositoryLocation::Sftp(location) => {
            let location = location.clone().with_env_overrides();
            sftp_storage(location).await
        }
    }
}

// =============================================================================
// Local Repository Storage
// =============================================================================

struct LocalRepositoryStorage {
    location: RepositoryLocation,
    root: PathBuf,
}

impl LocalRepositoryStorage {
    fn new(root: PathBuf) -> Self {
        Self {
            location: RepositoryLocation::Local(root.clone()),
            root,
        }
    }

    fn full_path(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

#[async_trait]
impl RepositoryStorage for LocalRepositoryStorage {
    fn location(&self) -> &RepositoryLocation {
        &self.location
    }

    async fn init(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(tokio::fs::try_exists(self.full_path(path)).await?)
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        Ok(tokio::fs::read(self.full_path(path)).await?.into())
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let full_path = self.full_path(path);
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(full_path, data).await?;
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        tokio::fs::remove_file(self.full_path(path)).await?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut results = Vec::new();
        let base = self.full_path(prefix);

        if !tokio::fs::try_exists(&base).await? {
            return Ok(results);
        }

        let mut entries = tokio::fs::read_dir(base).await?;
        while let Some(entry) = entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                results.push(name.to_string());
            }
        }

        Ok(results)
    }

    async fn metadata(&self, path: &str) -> Result<ObjectMetadata> {
        let metadata = tokio::fs::metadata(self.full_path(path)).await?;
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|duration| chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0))
            .unwrap_or_else(Utc::now);

        Ok(ObjectMetadata {
            size: metadata.len(),
            modified_at,
        })
    }
}

// =============================================================================
// S3 Repository Storage (AWS, Wasabi, Backblaze B2, MinIO)
// =============================================================================

struct S3RepositoryStorage {
    location: RepositoryLocation,
    config: S3Location,
    client: Client,
}

impl S3RepositoryStorage {
    async fn new(config: S3Location) -> Result<Self> {
        let mut loader = aws_config::defaults(BehaviorVersion::latest());
        if let Some(region) = &config.region {
            loader = loader.region(aws_config::Region::new(region.clone()));
        }
        if let Some(endpoint) = &config.endpoint {
            loader = loader.endpoint_url(endpoint.clone());
        }

        let shared = loader.load().await;
        let client = Client::new(&shared);

        Ok(Self {
            location: RepositoryLocation::S3(config.clone()),
            config,
            client,
        })
    }

    fn key(&self, path: &str) -> String {
        self.config.key(path)
    }
}

#[async_trait]
impl RepositoryStorage for S3RepositoryStorage {
    fn location(&self) -> &RepositoryLocation {
        &self.location
    }

    async fn init(&self) -> Result<()> {
        self.client
            .head_bucket()
            .bucket(&self.config.bucket)
            .send()
            .await
            .map_err(|e| {
                crate::Error::Backend(format!(
                    "Bucket {} not accessible: {}",
                    self.config.bucket, e
                ))
            })?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let result = self
            .client
            .head_object()
            .bucket(&self.config.bucket)
            .key(self.key(path))
            .send()
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(err) => {
                let message = err.to_string();
                if message.contains("NotFound") || message.contains("404") {
                    Ok(false)
                } else {
                    Err(crate::Error::Backend(format!(
                        "Failed to check existence: {}",
                        err
                    )))
                }
            }
        }
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        let response = self
            .client
            .get_object()
            .bucket(&self.config.bucket)
            .key(self.key(path))
            .send()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to read {}: {}", path, e)))?;

        let data =
            response.body.collect().await.map_err(|e| {
                crate::Error::Backend(format!("Failed to read {} body: {}", path, e))
            })?;

        Ok(data.into_bytes())
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let mut request = self
            .client
            .put_object()
            .bucket(&self.config.bucket)
            .key(self.key(path))
            .body(ByteStream::from(data.to_vec()));

        // Apply Server-Side Encryption if configured
        if let Some(ref sse) = self.config.sse {
            match sse.mode.as_str() {
                "aes256" => {
                    request = request.server_side_encryption(ServerSideEncryption::Aes256);
                }
                "kms" => {
                    request = request.server_side_encryption(ServerSideEncryption::AwsKms);
                    if let Some(ref key_id) = sse.kms_key_id {
                        request = request.ssekms_key_id(key_id);
                    }
                }
                _ => {}
            }
        }

        request
            .send()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to write {}: {}", path, e)))?;
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.config.bucket)
            .key(self.key(path))
            .send()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to delete {}: {}", path, e)))?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let key_prefix = self.key(prefix);
        let mut results = Vec::new();
        let mut continuation_token = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.config.bucket)
                .prefix(&key_prefix);

            if let Some(token) = continuation_token.take() {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|e| crate::Error::Backend(format!("Failed to list {}: {}", prefix, e)))?;

            if let Some(contents) = response.contents {
                for object in contents {
                    if let Some(key) = object.key {
                        let relative = if self.config.prefix.is_empty() {
                            key
                        } else {
                            key.strip_prefix(&format!("{}/", self.config.prefix))
                                .unwrap_or(&key)
                                .to_string()
                        };

                        // Strip the prefix directory from the relative path.
                        // For prefix "snapshots", we strip "snapshots/" to get just the filename.
                        let stripped = if prefix.is_empty() {
                            Some(relative.as_str())
                        } else {
                            relative.strip_prefix(&format!("{}/", prefix))
                        };

                        if let Some(name) = stripped {
                            // Only include direct children (no nested paths)
                            if !name.is_empty() && !name.contains('/') {
                                results.push(name.to_string());
                            }
                        }
                    }
                }
            }

            if response.is_truncated.unwrap_or(false) {
                continuation_token = response.next_continuation_token;
            } else {
                break;
            }
        }

        results.sort();
        results.dedup();
        Ok(results)
    }

    async fn metadata(&self, path: &str) -> Result<ObjectMetadata> {
        let response = self
            .client
            .head_object()
            .bucket(&self.config.bucket)
            .key(self.key(path))
            .send()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to stat {}: {}", path, e)))?;

        let modified_at = response
            .last_modified
            .map(|time| chrono::DateTime::from_timestamp(time.secs(), 0).unwrap_or_else(Utc::now))
            .unwrap_or_else(Utc::now);

        Ok(ObjectMetadata {
            size: response.content_length.unwrap_or(0) as u64,
            modified_at,
        })
    }
}

// =============================================================================
// Azure Blob Repository Storage
// =============================================================================

use azure_identity::DeveloperToolsCredential;
use azure_storage_blob::clients::BlobContainerClient;
use azure_storage_blob::models::{
    BlobClientGetPropertiesResultHeaders, BlobContainerClientListBlobsOptions,
};
use url::Url;

struct AzureRepositoryStorage {
    location: RepositoryLocation,
    config: AzureLocation,
    client: BlobContainerClient,
}

impl AzureRepositoryStorage {
    async fn new(config: AzureLocation) -> Result<Self> {
        let client = Self::build_container_client(&config)?;

        Ok(Self {
            location: RepositoryLocation::Azure(config.clone()),
            config,
            client,
        })
    }

    /// Build a container client.
    ///
    /// Authentication is resolved in this order:
    /// 1. SAS token from `AZURE_STORAGE_SAS_TOKEN` (or `AZURE_STORAGE_SAS`).
    /// 2. Microsoft Entra ID via the standard credential chain (env vars,
    ///    managed identity, Azure CLI, etc.).
    ///
    /// A custom endpoint may be supplied via `AZURE_STORAGE_ENDPOINT`
    /// (useful for sovereign clouds or Azurite); otherwise the public
    /// `https://<account>.blob.core.windows.net` endpoint is used.
    fn build_container_client(config: &AzureLocation) -> Result<BlobContainerClient> {
        let endpoint = std::env::var("AZURE_STORAGE_ENDPOINT").unwrap_or_else(|_| {
            format!("https://{}.blob.core.windows.net", config.account_name)
        });
        let endpoint = endpoint.trim_end_matches('/');

        if let Ok(sas) = std::env::var("AZURE_STORAGE_SAS_TOKEN")
            .or_else(|_| std::env::var("AZURE_STORAGE_SAS"))
        {
            let sas = sas.trim_start_matches('?');
            let url = Url::parse(&format!("{}/{}?{}", endpoint, config.container, sas))
                .map_err(|e| crate::Error::Backend(format!("Invalid Azure URL: {}", e)))?;
            return BlobContainerClient::new(url, None, None).map_err(|e| {
                crate::Error::Backend(format!("Failed to create Azure client: {}", e))
            });
        }

        let credential = DeveloperToolsCredential::new(None).map_err(|e| {
            crate::Error::Backend(format!(
                "Failed to create Azure credential (set AZURE_STORAGE_SAS_TOKEN for SAS auth, \
                 or configure Microsoft Entra ID): {}",
                e
            ))
        })?;
        let url = Url::parse(&format!("{}/{}", endpoint, config.container))
            .map_err(|e| crate::Error::Backend(format!("Invalid Azure URL: {}", e)))?;
        BlobContainerClient::new(url, Some(credential), None)
            .map_err(|e| crate::Error::Backend(format!("Failed to create Azure client: {}", e)))
    }

    fn key(&self, path: &str) -> String {
        self.config.key(path)
    }
}

#[async_trait]
impl RepositoryStorage for AzureRepositoryStorage {
    fn location(&self) -> &RepositoryLocation {
        &self.location
    }

    async fn init(&self) -> Result<()> {
        // Check if container exists, create if not
        match self.client.exists().await {
            Ok(true) => Ok(()),
            Ok(false) => {
                self.client.create(None).await.map_err(|e| {
                    crate::Error::Backend(format!("Failed to create container: {}", e))
                })?;
                Ok(())
            }
            Err(e) => Err(crate::Error::Backend(format!(
                "Failed to check container existence: {}",
                e
            ))),
        }
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let blob_client = self.client.blob_client(&self.key(path));
        match blob_client.exists().await {
            Ok(exists) => Ok(exists),
            Err(e) => Err(crate::Error::Backend(format!(
                "Failed to check existence: {}",
                e
            ))),
        }
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        let blob_client = self.client.blob_client(&self.key(path));

        let response = blob_client
            .download(None)
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to read {}: {}", path, e)))?;

        let body = response
            .body
            .collect()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to read body {}: {}", path, e)))?;

        Ok(body)
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let blob_client = self.client.blob_client(&self.key(path));

        blob_client
            .upload(data.into(), None)
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to write {}: {}", path, e)))?;

        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let blob_client = self.client.blob_client(&self.key(path));

        blob_client
            .delete(None)
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to delete {}: {}", path, e)))?;

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let full_prefix = self.key(prefix);
        let mut results = Vec::new();

        let options = BlobContainerClientListBlobsOptions {
            prefix: Some(full_prefix.clone()),
            ..Default::default()
        };

        let mut pager = self
            .client
            .list_blobs(Some(options))
            .map_err(|e| crate::Error::Backend(format!("Failed to list blobs: {}", e)))?;

        // The pager flattens pages into individual blob items.
        use futures::StreamExt;
        while let Some(blob) = pager.next().await {
            let blob = blob
                .map_err(|e| crate::Error::Backend(format!("Failed to list blobs: {}", e)))?;

            let Some(blob_name) = blob.name else {
                continue;
            };

            let relative = if self.config.prefix.is_empty() {
                blob_name.clone()
            } else {
                blob_name
                    .strip_prefix(&format!("{}/", self.config.prefix))
                    .unwrap_or(&blob_name)
                    .to_string()
            };

            // Strip the prefix directory from the relative path.
            let stripped = if prefix.is_empty() {
                Some(relative.as_str())
            } else {
                relative.strip_prefix(&format!("{}/", prefix))
            };

            if let Some(name) = stripped {
                // Only include direct children (no nested paths)
                if !name.is_empty() && !name.contains('/') {
                    results.push(name.to_string());
                }
            }
        }

        results.sort();
        results.dedup();
        Ok(results)
    }

    async fn metadata(&self, path: &str) -> Result<ObjectMetadata> {
        let blob_client = self.client.blob_client(&self.key(path));

        let response = blob_client
            .get_properties(None)
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to stat {}: {}", path, e)))?;

        let size = response.content_length().unwrap_or(None).unwrap_or(0);
        let modified_at = response
            .last_modified()
            .ok()
            .flatten()
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.unix_timestamp(), 0))
            .unwrap_or_else(Utc::now);

        Ok(ObjectMetadata { size, modified_at })
    }
}

// =============================================================================
// Rclone Repository Storage
// =============================================================================

use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

struct RcloneRepositoryStorage {
    location: RepositoryLocation,
    config: RcloneLocation,
    rclone_path: String,
}

impl RcloneRepositoryStorage {
    fn new(config: RcloneLocation) -> Self {
        let rclone_path = std::env::var("RCLONE_PATH").unwrap_or_else(|_| "rclone".to_string());
        Self {
            location: RepositoryLocation::Rclone(config.clone()),
            config,
            rclone_path,
        }
    }

    fn full_path(&self, path: &str) -> String {
        let key = self.config.key(path);
        format!("{}:{}", self.config.remote, key)
    }

    async fn run_rclone(&self, args: &[&str]) -> Result<(bool, Vec<u8>, String)> {
        let mut cmd = Command::new(&self.rclone_path);

        for arg in args {
            cmd.arg(arg);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            crate::Error::Backend(format!(
                "Failed to spawn rclone (is it installed?): {}",
                e
            ))
        })?;

        let mut stdout = Vec::new();
        let mut stderr = String::new();

        if let Some(ref mut stdout_pipe) = child.stdout {
            stdout_pipe.read_to_end(&mut stdout).await.map_err(|e| {
                crate::Error::Backend(format!("Failed to read rclone stdout: {}", e))
            })?;
        }

        if let Some(ref mut stderr_pipe) = child.stderr {
            // Read the full stderr stream; rclone emits multi-line diagnostics
            // and truncating to the first line hides the actual error cause.
            stderr_pipe.read_to_string(&mut stderr).await.ok();
        }

        let status = child
            .wait()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to wait for rclone: {}", e)))?;

        Ok((status.success(), stdout, stderr))
    }
}

#[async_trait]
impl RepositoryStorage for RcloneRepositoryStorage {
    fn location(&self) -> &RepositoryLocation {
        &self.location
    }

    async fn init(&self) -> Result<()> {
        // Verify rclone is available
        let (success, _, stderr) = self.run_rclone(&["version"]).await?;
        if !success {
            return Err(crate::Error::Backend(format!(
                "rclone not available: {}",
                stderr
            )));
        }

        // Create base directory
        let path = self.full_path("");
        let (success, _, stderr) = self.run_rclone(&["mkdir", &path]).await?;
        if !success && !stderr.contains("directory not empty") {
            return Err(crate::Error::Backend(format!(
                "Failed to create base directory: {}",
                stderr
            )));
        }

        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let full_path = self.full_path(path);
        let (success, stdout, _) = self.run_rclone(&["lsf", &full_path]).await?;
        Ok(success && !stdout.is_empty())
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        let full_path = self.full_path(path);
        let (success, stdout, stderr) = self.run_rclone(&["cat", &full_path]).await?;

        if !success {
            return Err(crate::Error::Backend(format!(
                "Failed to read {}: {}",
                path, stderr
            )));
        }

        Ok(Bytes::from(stdout))
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let full_path = self.full_path(path);

        // Write to a temp file first, then rclone copyto
        let temp_dir = tempfile::tempdir()
            .map_err(|e| crate::Error::Backend(format!("Failed to create temp dir: {}", e)))?;

        let temp_file = temp_dir.path().join("data");
        tokio::fs::write(&temp_file, &data)
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to write temp file: {}", e)))?;

        let temp_path = temp_file.to_string_lossy();
        let (success, _, stderr) = self
            .run_rclone(&["copyto", &temp_path, &full_path])
            .await?;

        if !success {
            return Err(crate::Error::Backend(format!(
                "Failed to write {}: {}",
                path, stderr
            )));
        }

        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full_path = self.full_path(path);
        let (success, _, stderr) = self.run_rclone(&["deletefile", &full_path]).await?;

        if !success {
            return Err(crate::Error::Backend(format!(
                "Failed to delete {}: {}",
                path, stderr
            )));
        }

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let full_path = self.full_path(prefix);
        let (success, stdout, stderr) = self
            .run_rclone(&["lsf", "--recursive", &full_path])
            .await?;

        if !success {
            // Empty directory is not an error
            if stderr.contains("directory not found") {
                return Ok(Vec::new());
            }
            return Err(crate::Error::Backend(format!(
                "Failed to list {}: {}",
                prefix, stderr
            )));
        }

        let files: Vec<String> = String::from_utf8_lossy(&stdout)
            .lines()
            .map(|line| line.trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty() && !s.contains('/'))
            .collect();

        Ok(files)
    }

    async fn metadata(&self, path: &str) -> Result<ObjectMetadata> {
        let full_path = self.full_path(path);
        let (success, stdout, stderr) = self.run_rclone(&["lsjson", &full_path]).await?;

        if !success {
            return Err(crate::Error::Backend(format!(
                "Failed to stat {}: {}",
                path, stderr
            )));
        }

        let json: Vec<serde_json::Value> = serde_json::from_slice(&stdout)
            .map_err(|e| crate::Error::Backend(format!("Failed to parse rclone output: {}", e)))?;

        if json.is_empty() {
            return Err(crate::Error::Backend(format!("File not found: {}", path)));
        }

        let item = &json[0];
        let size = item["Size"].as_u64().unwrap_or(0);
        let mod_time = item["ModTime"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        Ok(ObjectMetadata {
            size,
            modified_at: mod_time,
        })
    }
}

// =============================================================================
// SFTP Repository Storage
// =============================================================================

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{OpenFlags, StatusCode};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

/// SSH client handler that verifies the server host key against the local
/// `~/.ssh/known_hosts` file. Set `GHOSTSNAP_SFTP_INSECURE=1` to skip the check.
struct SftpClientHandler {
    host: String,
    port: u16,
    insecure: bool,
}

impl russh::client::Handler for SftpClientHandler {
    type Error = crate::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        if self.insecure {
            return Ok(true);
        }
        match russh::keys::check_known_hosts(&self.host, self.port, server_public_key) {
            Ok(true) => Ok(true),
            Ok(false) => Err(crate::Error::Backend(format!(
                "Host key for {}:{} is not in known_hosts. Add it with `ssh-keyscan` or set \
                 GHOSTSNAP_SFTP_INSECURE=1 to bypass verification.",
                self.host, self.port
            ))),
            Err(e) => Err(crate::Error::Backend(format!(
                "Host key verification failed for {}:{}: {}",
                self.host, self.port, e
            ))),
        }
    }
}

struct SftpRepositoryStorage {
    location: RepositoryLocation,
    config: SftpLocation,
    sftp: SftpSession,
    /// Keep the SSH session alive for the lifetime of the SFTP session.
    _session: russh::client::Handle<SftpClientHandler>,
    /// Cache of remote directories already created, to avoid redundant mkdirs.
    created_dirs: Mutex<HashSet<String>>,
}

impl SftpRepositoryStorage {
    async fn new(config: SftpLocation) -> Result<Self> {
        if config.user.is_empty() {
            return Err(crate::Error::Backend(
                "SFTP user is required (set it in the URI as sftp:user@host or via SFTP_USER)"
                    .to_string(),
            ));
        }

        let insecure = std::env::var("GHOSTSNAP_SFTP_INSECURE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let handler = SftpClientHandler {
            host: config.host.clone(),
            port: config.port,
            insecure,
        };

        let ssh_config = Arc::new(russh::client::Config::default());
        let mut session =
            russh::client::connect(ssh_config, (config.host.as_str(), config.port), handler)
                .await
                .map_err(|e| {
                    crate::Error::Backend(format!(
                        "Failed to connect to {}:{}: {}",
                        config.host, config.port, e
                    ))
                })?;

        Self::authenticate(&mut session, &config).await?;

        let channel = session.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to start SFTP session: {}", e)))?;

        Ok(Self {
            location: RepositoryLocation::Sftp(config.clone()),
            config,
            sftp,
            _session: session,
            created_dirs: Mutex::new(HashSet::new()),
        })
    }

    /// Authenticate using a private key (SFTP_KEY_FILE or the default
    /// `~/.ssh/id_ed25519` / `~/.ssh/id_ecdsa`) or a password (SFTP_PASSWORD).
    async fn authenticate(
        session: &mut russh::client::Handle<SftpClientHandler>,
        config: &SftpLocation,
    ) -> Result<()> {
        // Password auth takes precedence when explicitly provided.
        if let Ok(password) = std::env::var("SFTP_PASSWORD") {
            let result = session
                .authenticate_password(&config.user, password)
                .await?;
            if result.success() {
                return Ok(());
            }
            return Err(crate::Error::Backend(
                "SFTP password authentication failed".to_string(),
            ));
        }

        // Public-key auth: explicit key file or the conventional defaults.
        let passphrase = std::env::var("SFTP_KEY_PASSPHRASE").ok();
        let key_paths = Self::candidate_key_paths();
        for key_path in &key_paths {
            if !key_path.exists() {
                continue;
            }
            let key = russh::keys::load_secret_key(key_path, passphrase.as_deref())
                .map_err(|e| {
                    crate::Error::Backend(format!(
                        "Failed to load SSH key {}: {}",
                        key_path.display(),
                        e
                    ))
                })?;
            // Ed25519/ECDSA keys use their built-in hash, so no explicit
            // signature hash algorithm is needed. RSA is intentionally
            // unsupported (see docs/advisories).
            let key_with_alg =
                russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None);
            let result = session
                .authenticate_publickey(&config.user, key_with_alg)
                .await?;
            if result.success() {
                return Ok(());
            }
        }

        Err(crate::Error::Backend(format!(
            "SFTP authentication failed for {}@{}. Provide SFTP_PASSWORD, SFTP_KEY_FILE, or a \
             key at ~/.ssh/id_ed25519 / ~/.ssh/id_ecdsa.",
            config.user, config.host
        )))
    }

    fn candidate_key_paths() -> Vec<PathBuf> {
        if let Ok(explicit) = std::env::var("SFTP_KEY_FILE") {
            return vec![PathBuf::from(explicit)];
        }
        let mut paths = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            let ssh_dir = PathBuf::from(home).join(".ssh");
            paths.push(ssh_dir.join("id_ed25519"));
            paths.push(ssh_dir.join("id_ecdsa"));
        }
        paths
    }

    /// Recursively create the parent directories of `key` (mkdir -p), caching
    /// directories that have already been created.
    async fn ensure_parent_dirs(&self, key: &str) -> Result<()> {
        let Some(parent) = key.rsplit_once('/').map(|(p, _)| p) else {
            return Ok(());
        };
        if parent.is_empty() {
            return Ok(());
        }

        let mut cache = self.created_dirs.lock().await;
        let mut current = String::new();
        for component in parent.split('/') {
            if component.is_empty() {
                continue;
            }
            if current.is_empty() {
                current.push_str(component);
            } else {
                current.push('/');
                current.push_str(component);
            }
            if cache.contains(&current) {
                continue;
            }
            match self.sftp.create_dir(current.clone()).await {
                Ok(()) => {}
                Err(russh_sftp::client::error::Error::Status(status))
                    if status.status_code == StatusCode::Failure =>
                {
                    // Already exists (servers report mkdir-on-existing as Failure).
                }
                Err(e) => {
                    return Err(crate::Error::Backend(format!(
                        "Failed to create remote directory {}: {}",
                        current, e
                    )));
                }
            }
            cache.insert(current.clone());
        }
        Ok(())
    }
}

#[async_trait]
impl RepositoryStorage for SftpRepositoryStorage {
    fn location(&self) -> &RepositoryLocation {
        &self.location
    }

    async fn init(&self) -> Result<()> {
        let base = self.config.key("");
        let base = base.trim_end_matches('/');
        if base.is_empty() {
            return Ok(());
        }
        // Create the base directory tree by ensuring the parents of a sentinel
        // child path, then the base itself.
        self.ensure_parent_dirs(&format!("{}/x", base)).await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let key = self.config.key(path);
        self.sftp
            .try_exists(key)
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to stat {}: {}", path, e)))
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        let key = self.config.key(path);
        let data = self.sftp.read(key).await.map_err(|e| match e {
            russh_sftp::client::error::Error::Status(status)
                if status.status_code == StatusCode::NoSuchFile =>
            {
                crate::Error::ChunkNotFound {
                    id: path.to_string(),
                }
            }
            other => crate::Error::Backend(format!("Failed to read {}: {}", path, other)),
        })?;
        Ok(Bytes::from(data))
    }

    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        let key = self.config.key(path);
        self.ensure_parent_dirs(&key).await?;

        let mut file = self
            .sftp
            .open_with_flags(
                key.clone(),
                OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            )
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to open {}: {}", path, e)))?;

        use tokio::io::AsyncWriteExt;
        file.write_all(&data)
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to write {}: {}", path, e)))?;
        file.shutdown()
            .await
            .map_err(|e| crate::Error::Backend(format!("Failed to flush {}: {}", path, e)))?;
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let key = self.config.key(path);
        match self.sftp.remove_file(key).await {
            Ok(()) => Ok(()),
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == StatusCode::NoSuchFile =>
            {
                Ok(())
            }
            Err(e) => Err(crate::Error::Backend(format!(
                "Failed to delete {}: {}",
                path, e
            ))),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let key = self.config.key(prefix);
        let dir = if key.is_empty() { ".".to_string() } else { key };
        let read_dir = match self.sftp.read_dir(dir).await {
            Ok(entries) => entries,
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == StatusCode::NoSuchFile =>
            {
                return Ok(Vec::new());
            }
            Err(e) => {
                return Err(crate::Error::Backend(format!(
                    "Failed to list {}: {}",
                    prefix, e
                )));
            }
        };

        let names = read_dir.map(|entry| entry.file_name()).collect();
        Ok(names)
    }

    async fn metadata(&self, path: &str) -> Result<ObjectMetadata> {
        let key = self.config.key(path);
        let meta = self.sftp.metadata(key).await.map_err(|e| match e {
            russh_sftp::client::error::Error::Status(status)
                if status.status_code == StatusCode::NoSuchFile =>
            {
                crate::Error::ChunkNotFound {
                    id: path.to_string(),
                }
            }
            other => crate::Error::Backend(format!("Failed to stat {}: {}", path, other)),
        })?;

        let size = meta.size.unwrap_or(0);
        let modified_at = meta
            .modified()
            .ok()
            .and_then(|t| chrono::DateTime::<Utc>::from_timestamp(
                t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64,
                0,
            ))
            .unwrap_or_else(Utc::now);

        Ok(ObjectMetadata { size, modified_at })
    }
}
