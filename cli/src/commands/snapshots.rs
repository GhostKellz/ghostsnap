use anyhow::{Result, anyhow};
use clap::Args;
use ghostsnap_core::{NodeType, Repository};
use std::io::{self, Write};
use tracing::info;

#[derive(Args)]
pub struct SnapshotsCommand {
    #[arg(long, help = "Output format (table, json)")]
    format: Option<String>,

    #[arg(long, help = "Filter by hostname")]
    hostname: Option<String>,

    #[arg(long, help = "Filter by tag")]
    tag: Vec<String>,

    #[arg(long, help = "Show latest N snapshots")]
    latest: Option<usize>,
}

impl SnapshotsCommand {
    pub async fn run(&self, cli: &crate::Cli) -> Result<()> {
        let repo_location = crate::commands::parse_repository_location(cli.repo.as_ref())?;

        let password = cli
            .password
            .clone()
            .or_else(|| {
                print!("Enter repository password: ");
                io::stdout().flush().ok()?;
                rpassword::read_password().ok()
            })
            .ok_or_else(|| anyhow!("Password required"))?;

        info!("Opening repository at: {}", repo_location.display());
        let repo = Repository::open_at_location(repo_location, &password).await?;

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
            snapshots.sort_by_key(|s| std::cmp::Reverse(s.time));
            snapshots.truncate(latest);
        }

        match format {
            "table" => {
                println!(
                    "{:<12} {:<20} {:<15} {:<6} {:<20} Paths",
                    "ID", "Date", "Host", "Files", "Tags"
                );
                println!("{:-<100}", "");

                for snapshot in snapshots {
                    let tags_str = snapshot.tags.join(",");
                    let paths_str = snapshot
                        .paths
                        .iter()
                        .map(|p| p.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(",");

                    // Load tree to count actual files
                    let file_count = if let Ok(tree) = repo.load_tree(&snapshot.tree).await {
                        tree.nodes
                            .iter()
                            .filter(|n| n.node_type == NodeType::File)
                            .count()
                    } else {
                        0
                    };

                    println!(
                        "{:<12} {:<20} {:<15} {:<6} {:<20} {}",
                        snapshot.short_id(),
                        snapshot.time.format("%Y-%m-%d %H:%M:%S"),
                        snapshot.hostname,
                        file_count,
                        tags_str,
                        paths_str
                    );
                }
            }
            "json" => {
                let json = serde_json::to_string_pretty(&snapshots)?;
                println!("{}", json);
            }
            _ => {
                return Err(anyhow!("Unsupported format: {}", format));
            }
        }

        Ok(())
    }
}
