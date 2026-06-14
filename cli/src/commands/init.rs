use anyhow::{Result, anyhow};
use clap::{Args, ValueEnum};
use ghostsnap_backends::{AzureBackend, Backend, LocalBackend, S3SseConfig, SseType};
use ghostsnap_core::Repository;
use ghostsnap_core::S3RepoSse;
use ghostsnap_core::storage::{AzureLocation, RcloneLocation, RepositoryLocation, S3Location};
use std::io::{self, Write};
use tracing::info;

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum S3SseType {
    #[default]
    None,
    /// AES256 server-side encryption (SSE-S3)
    Aes256,
    /// AWS KMS server-side encryption (SSE-KMS)
    Kms,
}

impl From<S3SseType> for SseType {
    fn from(value: S3SseType) -> Self {
        match value {
            S3SseType::None => SseType::None,
            S3SseType::Aes256 => SseType::Aes256,
            S3SseType::Kms => SseType::Kms,
        }
    }
}

#[derive(Args)]
pub struct InitCommand {
    #[arg(help = "Repository path")]
    repo: Option<String>,

    #[arg(long, help = "Backend type (local, s3, b2, minio, azure, rclone). Inferred from the URI scheme when omitted.")]
    backend: Option<String>,

    // S3 options
    #[arg(long, help = "S3 bucket name")]
    bucket: Option<String>,

    #[arg(long, help = "S3 key prefix")]
    prefix: Option<String>,

    #[arg(long, help = "S3 endpoint URL (for S3-compatible storage like MinIO, Wasabi)")]
    endpoint: Option<String>,

    #[arg(long, help = "S3 region", default_value = "us-east-1")]
    region: String,

    // S3 Server-Side Encryption options
    #[arg(
        long,
        value_enum,
        help = "S3 Server-Side Encryption type (none, aes256, kms)"
    )]
    sse_type: Option<S3SseType>,

    #[arg(long, help = "KMS key ID for SSE-KMS encryption")]
    sse_kms_key_id: Option<String>,

    // Azure options
    #[arg(long, help = "Azure container name")]
    container: Option<String>,

    #[arg(long, help = "Azure storage account name")]
    account_name: Option<String>,

    #[arg(long, help = "Azure blob prefix")]
    azure_prefix: Option<String>,

    // Rclone options
    #[arg(long, help = "Rclone remote name (e.g., 'myremote', 'gdrive')")]
    remote: Option<String>,

    #[arg(long, help = "Rclone path within the remote")]
    rclone_path: Option<String>,
}

impl InitCommand {
    pub async fn run(&self, cli: &crate::Cli) -> Result<()> {
        let cli_backend = self.backend.as_deref().unwrap_or("local");

        // For Azure backend, we can construct the repo URI from flags
        // For other backends, repo is required
        let repo_input = match cli_backend {
            "azure" if self.account_name.is_some() && self.container.is_some() => {
                // Construct Azure URI from flags if no repo given
                self.repo.clone().or_else(|| cli.repo.clone()).or_else(|| {
                    let account = self.account_name.as_ref().unwrap();
                    let container = self.container.as_ref().unwrap();
                    let prefix = self.azure_prefix.as_deref().unwrap_or("");
                    if prefix.is_empty() {
                        Some(format!("azure:{}/{}", account, container))
                    } else {
                        Some(format!("azure:{}/{}/{}", account, container, prefix))
                    }
                })
            }
            scheme @ ("s3" | "b2" | "minio") if self.bucket.is_some() => {
                // Construct an S3-compatible URI from flags if no repo given.
                self.repo.clone().or_else(|| cli.repo.clone()).or_else(|| {
                    let bucket = self.bucket.as_ref().unwrap();
                    let prefix = self.prefix.as_deref().unwrap_or("");
                    if prefix.is_empty() {
                        Some(format!("{}:{}", scheme, bucket))
                    } else {
                        Some(format!("{}:{}/{}", scheme, bucket, prefix))
                    }
                })
            }
            "rclone" if self.remote.is_some() => {
                // Construct rclone URI from flags if no repo given
                self.repo.clone().or_else(|| cli.repo.clone()).or_else(|| {
                    let remote = self.remote.as_ref().unwrap();
                    let path = self.rclone_path.as_deref().unwrap_or("");
                    if path.is_empty() {
                        Some(format!("rclone:{}", remote))
                    } else {
                        Some(format!("rclone:{}/{}", remote, path))
                    }
                })
            }
            _ => self.repo.clone().or_else(|| cli.repo.clone()),
        };

        let repo_input = repo_input
            .ok_or_else(|| anyhow!("Repository path required (--repo or GHOSTSNAP_REPO)"))?;

        // When --backend is omitted, infer it from the URI scheme so that
        // `ghostsnap init s3:bucket` (or b2:/minio:/azure:/rclone:) just works.
        let backend_type = if self.backend.is_some() {
            cli_backend.to_string()
        } else {
            infer_backend_from_uri(&repo_input)
        };
        let backend_type = backend_type.as_str();

        let password = cli
            .password
            .clone()
            .or_else(|| {
                print!("Enter repository password: ");
                io::stdout().flush().ok()?;
                rpassword::read_password().ok()
            })
            .ok_or_else(|| anyhow!("Password required"))?;

        info!("Initializing repository at: {}", repo_input);

        match backend_type {
            "local" => {
                let repo_location =
                    RepositoryLocation::parse(&repo_input).map_err(|e| anyhow!(e.to_string()))?;
                match &repo_location {
                    RepositoryLocation::Local(path) => {
                        let _backend = LocalBackend::new(path);
                    }
                    RepositoryLocation::S3(_) => {
                        return Err(anyhow!(
                            "Use `--backend s3` when initializing an S3 repository URI"
                        ));
                    }
                    RepositoryLocation::Azure(_) => {
                        return Err(anyhow!(
                            "Use `--backend azure` when initializing an Azure repository URI"
                        ));
                    }
                    RepositoryLocation::Rclone(_) => {
                        return Err(anyhow!(
                            "Use `--backend rclone` when initializing an rclone repository URI"
                        ));
                    }
                    RepositoryLocation::Sftp(_) => {
                        return Err(anyhow!(
                            "Use `--backend sftp` when initializing an SFTP repository URI"
                        ));
                    }
                }
                let _repo = Repository::init_at_location(repo_location.clone(), &password).await?;
                println!(
                    "Successfully initialized repository at {}",
                    repo_location.display()
                );
            }

            "s3" | "b2" | "minio" => {
                // Resolve the S3-compatible location from the URI. b2:/minio:
                // URIs map to an S3 location with an endpoint resolved from the
                // environment; flags below act as explicit overrides.
                let mut location = match RepositoryLocation::parse(&repo_input)
                    .map_err(|e| anyhow!(e.to_string()))?
                {
                    RepositoryLocation::S3(location) => location,
                    RepositoryLocation::Local(_) => {
                        let bucket = self.bucket.clone().ok_or_else(|| {
                            anyhow!("S3 bucket required: pass an s3:/b2:/minio: URI or --bucket")
                        })?;
                        S3Location::new(bucket, self.prefix.clone().unwrap_or_default())
                    }
                    RepositoryLocation::Azure(_) => {
                        return Err(anyhow!("Use `--backend azure` for Azure repository URIs"));
                    }
                    RepositoryLocation::Rclone(_) => {
                        return Err(anyhow!("Use `--backend rclone` for rclone repository URIs"));
                    }
                    RepositoryLocation::Sftp(_) => {
                        return Err(anyhow!("Use `--backend sftp` for SFTP repository URIs"));
                    }
                };

                // Apply flag overrides only when explicitly provided so that
                // endpoint/region resolved from the URI scheme are preserved.
                if let Some(bucket) = &self.bucket {
                    location.bucket = bucket.clone();
                }
                if let Some(prefix) = &self.prefix {
                    location.prefix = prefix.clone();
                }
                if let Some(endpoint) = &self.endpoint {
                    location.endpoint = Some(endpoint.clone());
                }
                if location.region.is_none() {
                    location.region = Some(self.region.clone());
                }
                if location.bucket.is_empty() {
                    return Err(anyhow!("S3 bucket required (--bucket or a bucket in the URI)"));
                }

                // Build SSE configuration
                let sse_config = S3SseConfig {
                    sse_type: self.sse_type.unwrap_or_default().into(),
                    kms_key_id: self.sse_kms_key_id.clone(),
                };

                let repo_location = RepositoryLocation::S3(location.clone());
                let mut repo =
                    Repository::init_at_location(repo_location.clone(), &password).await?;
                let persisted_sse = match sse_config.sse_type {
                    SseType::None => None,
                    SseType::Aes256 => Some(S3RepoSse {
                        mode: "aes256".to_string(),
                        kms_key_id: None,
                    }),
                    SseType::Kms => Some(S3RepoSse {
                        mode: "kms".to_string(),
                        kms_key_id: sse_config.kms_key_id.clone(),
                    }),
                };
                repo.set_s3_transport_config(&location, persisted_sse)
                    .await?;

                let sse_info = match sse_config.sse_type {
                    SseType::None => String::new(),
                    SseType::Aes256 => " (SSE-S3/AES256)".to_string(),
                    SseType::Kms => {
                        if let Some(ref key_id) = sse_config.kms_key_id {
                            format!(" (SSE-KMS: {})", key_id)
                        } else {
                            " (SSE-KMS: default key)".to_string()
                        }
                    }
                };
                println!(
                    "Successfully initialized {} repository at {} (bucket: {} prefix: {}{})",
                    backend_type,
                    repo_location.display(),
                    location.bucket,
                    if location.prefix.is_empty() {
                        "<root>"
                    } else {
                        &location.prefix
                    },
                    sse_info
                );
            }

            "azure" => {
                let container = self
                    .container
                    .as_ref()
                    .ok_or_else(|| anyhow!("Azure container required (--container)"))?;
                let account_name = self
                    .account_name
                    .as_ref()
                    .ok_or_else(|| anyhow!("Azure account name required (--account-name)"))?;
                let prefix = self.azure_prefix.as_deref().unwrap_or("");

                // Validate Azure credentials by creating backend
                println!("Validating Azure credentials...");
                let backend = AzureBackend::new(account_name.clone(), container.clone())
                    .await
                    .map_err(|e| anyhow!("Azure authentication failed: {}", e))?;

                // Set prefix if provided
                let _backend = if !prefix.is_empty() {
                    backend.with_prefix(prefix.to_string())
                } else {
                    backend
                };

                // Create Azure location
                let azure_location = AzureLocation::new(
                    account_name.clone(),
                    container.clone(),
                    prefix.to_string(),
                );
                let repo_location = RepositoryLocation::Azure(azure_location);

                // Initialize the repository
                let _repo = Repository::init_at_location(repo_location.clone(), &password).await?;

                println!(
                    "Successfully initialized Azure repository at {} (account: {} container: {} prefix: {})",
                    repo_location.display(),
                    account_name,
                    container,
                    if prefix.is_empty() { "<root>" } else { prefix }
                );
            }

            "rclone" => {
                let remote = self
                    .remote
                    .as_ref()
                    .ok_or_else(|| anyhow!("Rclone remote required (--remote)"))?;
                let path = self.rclone_path.as_deref().unwrap_or("");

                // Validate rclone is available and remote exists
                println!("Validating rclone remote '{}'...", remote);
                let backend = ghostsnap_backends::RcloneBackend::new(remote.clone(), path.to_string());

                // Validate connectivity by checking if we can list the path
                backend.list("")
                    .await
                    .map_err(|e| anyhow!("Rclone validation failed: {}. Is rclone installed and is '{}' configured?", e, remote))?;

                // Create rclone location
                let rclone_location = RcloneLocation::new(remote.clone(), path.to_string());
                let repo_location = RepositoryLocation::Rclone(rclone_location);

                // Initialize the repository
                let _repo = Repository::init_at_location(repo_location.clone(), &password).await?;

                println!(
                    "Successfully initialized rclone repository at {} (remote: {} path: {})",
                    repo_location.display(),
                    remote,
                    if path.is_empty() { "<root>" } else { path }
                );
            }

            "sftp" => {
                // Resolve the SFTP location from the URI, applying user/host
                // overrides from flags and environment where provided.
                let location = match RepositoryLocation::parse(&repo_input)
                    .map_err(|e| anyhow!(e.to_string()))?
                {
                    RepositoryLocation::Sftp(location) => location.with_env_overrides(),
                    _ => {
                        return Err(anyhow!(
                            "SFTP repository URI required: sftp:[user@]host[:port][/path]"
                        ));
                    }
                };

                println!("Connecting to {}@{}...", location.user, location.host);
                let repo_location = RepositoryLocation::Sftp(location.clone());
                let _repo = Repository::init_at_location(repo_location.clone(), &password).await?;

                println!(
                    "Successfully initialized SFTP repository at {} (host: {} user: {} path: {})",
                    repo_location.display(),
                    location.host,
                    location.user,
                    if location.path.is_empty() {
                        "<login dir>"
                    } else {
                        &location.path
                    }
                );
            }

            _ => {
                return Err(anyhow!(
                    "Unsupported backend type: {}. Supported: local, s3, b2, minio, azure, rclone, sftp",
                    backend_type
                ));
            }
        }

        Ok(())
    }
}

/// Infer the backend type from a repository URI scheme.
///
/// Returns `local` for plain filesystem paths (including Windows-style paths
/// whose first colon is a drive letter rather than a known scheme).
fn infer_backend_from_uri(uri: &str) -> String {
    for scheme in ["s3", "b2", "minio", "azure", "rclone", "sftp"] {
        if uri.starts_with(&format!("{}:", scheme)) {
            return scheme.to_string();
        }
    }
    "local".to_string()
}
