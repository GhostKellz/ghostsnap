use anyhow::{anyhow, Result};
use clap::Args;
use ghostsnap_backends::{
    Backend, LocalBackend, AzureSimpleBackend,
    MinIOBackend, MinIOConfig
};
use ghostsnap_core::Repository;
use std::io::{self, Write};
use tracing::info;

#[derive(Args)]
pub struct InitCommand {
    #[arg(help = "Repository path")]
    repo: Option<String>,
    
    #[arg(long, help = "Backend type (local, s3, azure, minio, b2)")]
    backend: Option<String>,
    
    // S3/MinIO options
    #[arg(long, help = "S3/MinIO bucket name")]
    bucket: Option<String>,
    
    #[arg(long, help = "S3/MinIO prefix")]
    prefix: Option<String>,
    
    #[arg(long, help = "S3/MinIO endpoint URL")]
    endpoint: Option<String>,
    
    #[arg(long, help = "Access key for S3/MinIO")]
    access_key: Option<String>,
    
    #[arg(long, help = "Secret key for S3/MinIO")]
    secret_key: Option<String>,
    
    #[arg(long, help = "Region for S3/MinIO", default_value = "us-east-1")]
    region: String,
    
    // Azure options
    #[arg(long, help = "Azure container name")]
    container: Option<String>,
    
    #[arg(long, help = "Azure storage account name")]
    account_name: Option<String>,
    
    #[arg(long, help = "Azure connection string")]
    connection_string: Option<String>,
    
    #[arg(long, help = "Azure client ID for managed identity")]
    client_id: Option<String>,
    
    #[arg(long, help = "Azure storage tier (hot, cool, archive)")]
    storage_tier: Option<String>,
}

impl InitCommand {
    pub async fn run(&self, cli: &crate::Cli) -> Result<()> {
        let repo_path = self.repo.as_ref()
            .or(cli.repo.as_ref())
            .ok_or_else(|| anyhow!("Repository path required (--repo or GHOSTSNAP_REPO)"))?;
        
        let password = cli.password.as_ref()
            .map(|p| p.clone())
            .or_else(|| {
                print!("Enter repository password: ");
                io::stdout().flush().ok()?;
                rpassword::read_password().ok()
            })
            .ok_or_else(|| anyhow!("Password required"))?;
        
        info!("Initializing repository at: {}", repo_path);
        
        let backend_type = self.backend.as_deref().unwrap_or("local");
        
        match backend_type {
            "local" => {
                let _backend = LocalBackend::new(repo_path);
                let _repo = Repository::init(repo_path, &password).await?;
                println!("Successfully initialized local repository at {}", repo_path);
            },
            
            "s3" => {
                let bucket = self.bucket.as_ref()
                    .ok_or_else(|| anyhow!("S3 bucket required (--bucket)"))?;
                let prefix = self.prefix.as_deref().unwrap_or("");
                
                if let Some(endpoint) = &self.endpoint {
                    let _backend = ghostsnap_backends::S3Backend::with_endpoint(
                        bucket.clone(),
                        prefix.to_string(),
                        endpoint.clone(),
                    ).await?;
                } else {
                    let _backend = ghostsnap_backends::S3Backend::new(
                        bucket.clone(),
                        prefix.to_string(),
                    ).await?;
                }
                
                let _repo = Repository::init(repo_path, &password).await?;
                println!("Successfully initialized S3 repository: s3://{}/{}", bucket, prefix);
            },
            
            "minio" => {
                let bucket = self.bucket.as_ref()
                    .ok_or_else(|| anyhow!("MinIO bucket required (--bucket)"))?;
                let endpoint = self.endpoint.as_ref()
                    .ok_or_else(|| anyhow!("MinIO endpoint required (--endpoint)"))?;
                let access_key = self.access_key.as_ref()
                    .ok_or_else(|| anyhow!("MinIO access key required (--access-key)"))?;
                let secret_key = self.secret_key.as_ref()
                    .ok_or_else(|| anyhow!("MinIO secret key required (--secret-key)"))?;
                
                let mut config = MinIOConfig {
                    endpoint: endpoint.clone(),
                    access_key: access_key.clone(),
                    secret_key: secret_key.clone(),
                    bucket: bucket.clone(),
                    prefix: self.prefix.as_deref().unwrap_or("").to_string(),
                    region: self.region.clone(),
                    ..Default::default()
                };
                
                // Enable SSL if endpoint uses https
                config.use_ssl = endpoint.starts_with("https://");
                
                let _backend = MinIOBackend::new(config).await?;
                let _repo = Repository::init(repo_path, &password).await?;
                println!("Successfully initialized MinIO repository: {}/{}", endpoint, bucket);
            },
            
            "azure" => {
                let container = self.container.as_ref()
                    .ok_or_else(|| anyhow!("Azure container required (--container)"))?;
                let account_name = self.account_name.as_ref()
                    .ok_or_else(|| anyhow!("Azure account name required (--account-name)"))?;
                
                let _backend = AzureSimpleBackend::new(account_name.clone(), container.clone());
                let _repo = Repository::init(repo_path, &password).await?;
                println!("Successfully initialized Azure repository: {}/{}", account_name, container);
            },
            
            _ => {
                return Err(anyhow!("Unsupported backend type: {}. Supported: local, s3, minio, azure", backend_type));
            }
        }
        
        Ok(())
    }
}