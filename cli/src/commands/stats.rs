use anyhow::{Result, anyhow};
use clap::Args;
use ghostsnap_core::Repository;
use std::io::{self, Write};

#[derive(Args)]
pub struct StatsCommand {
    #[arg(long, help = "Output in JSON format")]
    json: bool,
}

impl StatsCommand {
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

        let repo = Repository::open_at_location(repo_location.clone(), &password).await?;

        // Get snapshot count
        let snapshots = repo.list_snapshots().await?;
        let snapshot_count = snapshots.len();

        // Get pack files and calculate sizes
        let packs = repo.list_packs().await?;
        let pack_count = packs.len();

        let mut total_pack_size = 0u64;

        for pack_id in &packs {
            if let Ok(size) = repo.pack_size(pack_id).await {
                total_pack_size += size;
            }
        }

        // Get index stats
        let index = repo.index();
        let index_guard = index.read().await;
        let chunk_count = index_guard.chunk_count();

        // Calculate dedup ratio from snapshots
        let mut total_original_size = 0u64;
        for snapshot_id in &snapshots {
            if let Ok(snapshot) = repo.load_snapshot(snapshot_id).await
                && let Ok(tree) = repo.load_tree(&snapshot.tree).await
            {
                total_original_size += tree.total_size();
            }
        }

        let dedup_ratio = if total_pack_size > 0 {
            total_original_size as f64 / total_pack_size as f64
        } else {
            1.0
        };

        if self.json {
            let stats = serde_json::json!({
                "repository": repo_location.display(),
                "snapshots": snapshot_count,
                "packs": pack_count,
                "chunks": chunk_count,
                "total_size_bytes": total_pack_size,
                "original_size_bytes": total_original_size,
                "dedup_ratio": dedup_ratio,
            });
            println!("{}", serde_json::to_string_pretty(&stats)?);
        } else {
            println!("Repository Statistics");
            println!("=====================");
            println!();
            println!("Location:     {}", repo_location.display());
            println!("Snapshots:    {}", snapshot_count);
            println!();
            println!("Storage:");
            println!("  Packs:      {}", pack_count);
            println!("  Chunks:     {}", chunk_count);
            println!("  Size:       {}", format_size(total_pack_size));
            println!();
            println!("Deduplication:");
            println!("  Original:   {}", format_size(total_original_size));
            println!("  Stored:     {}", format_size(total_pack_size));
            println!("  Ratio:      {:.2}x", dedup_ratio);
            println!(
                "  Saved:      {}",
                format_size(total_original_size.saturating_sub(total_pack_size))
            );
        }

        Ok(())
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
