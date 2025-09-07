use anyhow::{anyhow, Result};
use clap::Args;
use ghostsnap_core::Repository;
use std::io::{self, Write};
use tracing::info;

#[derive(Args)]
pub struct SnapshotsCommand {
    #[arg(long, help = "Group snapshots by this field")]
    group_by: Option<String>,
    
    #[arg(long, help = "Output format (table, json)")]
    format: Option<String>,
    
    #[arg(long, help = "Filter by hostname")]
    hostname: Option<String>,
    
    #[arg(long, help = "Filter by tag")]
    tag: Vec<String>,
    
    #[arg(long, help = "Filter by path")]
    path: Vec<String>,
    
    #[arg(long, help = "Show latest N snapshots")]
    latest: Option<usize>,
}

impl SnapshotsCommand {
    pub async fn run(&self, cli: &crate::Cli) -> Result<()> {
        let repo_path = cli.repo.as_ref()
            .ok_or_else(|| anyhow!("Repository path required (--repo or GHOSTSNAP_REPO)"))?;
        
        let password = cli.password.as_ref()
            .map(|p| p.clone())
            .or_else(|| {
                print!("Enter repository password: ");
                io::stdout().flush().ok()?;
                rpassword::read_password().ok()
            })
            .ok_or_else(|| anyhow!("Password required"))?;
        
        info!("Opening repository at: {}", repo_path);
        let repo = Repository::open(repo_path, &password).await?;
        
        let snapshot_ids = repo.list_snapshots().await?;
        let format = self.format.as_deref().unwrap_or("table");
        
        if snapshot_ids.is_empty() {
            println!("No snapshots found");
            return Ok(());
        }
        
        let mut snapshots = Vec::new();
        for snapshot_id in snapshot_ids {
            if let Ok(snapshot) = repo.load_snapshot(&snapshot_id).await {
                snapshots.push(snapshot);
            }
        }
        
        // Apply filters
        if let Some(hostname_filter) = &self.hostname {
            snapshots.retain(|s| s.hostname == *hostname_filter);
        }
        
        if !self.tag.is_empty() {
            snapshots.retain(|s| s.tags.iter().any(|tag| self.tag.contains(tag)));
        }
        
        // Apply latest limit
        if let Some(latest) = self.latest {
            snapshots.sort_by(|a, b| b.time.cmp(&a.time));
            snapshots.truncate(latest);
        }
        
        match format {
            "table" => {
                println!("{:<12} {:<20} {:<15} {:<6} {:<20} {}", 
                    "ID", "Date", "Host", "Files", "Tags", "Paths");
                println!("{:-<100}", "");
                
                for snapshot in snapshots {
                    let tags_str = snapshot.tags.join(",");
                    let paths_str = snapshot.paths.iter()
                        .map(|p| p.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(",");
                    
                    println!("{:<12} {:<20} {:<15} {:<6} {:<20} {}", 
                        snapshot.short_id(),
                        snapshot.time.format("%Y-%m-%d %H:%M:%S"),
                        snapshot.hostname,
                        snapshot.paths.len(),
                        tags_str,
                        paths_str
                    );
                }
            },
            "json" => {
                let json = serde_json::to_string_pretty(&snapshots)?;
                println!("{}", json);
            },
            _ => {
                return Err(anyhow!("Unsupported format: {}", format));
            }
        }
        
        Ok(())
    }
}