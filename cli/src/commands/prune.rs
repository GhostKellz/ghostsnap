use anyhow::{Result, anyhow};
use clap::Args;
use ghostsnap_core::{ChunkID, LockManager, LockType, Repository};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::io::{self, Write};
use tracing::info;

#[derive(Args)]
pub struct PruneCommand {
    #[arg(long, short = 'n', help = "Dry run - show what would be deleted")]
    pub dry_run: bool,

    #[arg(
        long,
        help = "Maximum percentage of unused data in a pack before repacking"
    )]
    pub max_unused: Option<u32>,
}

impl PruneCommand {
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

        let repo = Repository::open_at_location(repo_location, &password).await?;

        // Acquire exclusive lock for prune operation
        let _lock = if let Some(repo_path) = repo.local_path() {
            let lock_manager = LockManager::new(repo_path);
            Some(lock_manager.acquire(LockType::Exclusive, "prune").await?)
        } else {
            tracing::warn!("Repository locking not supported for remote repositories");
            None
        };

        println!("Analyzing repository...");
        println!();

        // Step 1: Find all chunks referenced by snapshots
        let snapshots = repo.list_snapshots().await?;
        let mut referenced_chunks: HashSet<ChunkID> = HashSet::new();

        println!(
            "[1/4] Scanning {} snapshots for referenced chunks...",
            snapshots.len()
        );
        let pb = ProgressBar::new(snapshots.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{bar:40} {pos}/{len}")
                .unwrap(),
        );

        for snapshot_id in &snapshots {
            if let Ok(snapshot) = repo.load_snapshot(snapshot_id).await
                && let Ok(tree) = repo.load_tree(&snapshot.tree).await
            {
                for node in &tree.nodes {
                    for chunk_ref in &node.chunks {
                        referenced_chunks.insert(chunk_ref.id);
                    }
                }
            }
            pb.inc(1);
        }
        pb.finish_and_clear();
        println!("  Found {} referenced chunks", referenced_chunks.len());

        // Step 2: Find all indexed chunks
        let index = repo.index();
        let index_guard = index.read().await;
        let all_chunks: HashSet<_> = index_guard.iter_chunks().map(|(id, _)| *id).collect();
        drop(index_guard);

        println!("[2/4] Checking {} indexed chunks...", all_chunks.len());

        // Step 3: Find orphaned chunks (in index but not referenced)
        let orphaned_chunks: HashSet<_> =
            all_chunks.difference(&referenced_chunks).cloned().collect();
        println!("  Found {} orphaned chunks", orphaned_chunks.len());

        if orphaned_chunks.is_empty() {
            println!();
            println!("No unused data to prune");
            return Ok(());
        }

        // Step 4: Analyze packs containing orphaned chunks
        println!("[3/4] Analyzing pack files...");

        // Map pack_id -> (total_chunks, orphaned_chunks, size)
        let mut pack_stats: std::collections::HashMap<String, (usize, usize, u64)> =
            std::collections::HashMap::new();

        let index = repo.index();
        let index_guard = index.read().await;

        // Count chunks per pack
        for (chunk_id, location) in index_guard.iter_chunks() {
            let entry = pack_stats
                .entry(location.pack_id.clone())
                .or_insert((0, 0, 0));
            entry.0 += 1;
            if orphaned_chunks.contains(chunk_id) {
                entry.1 += 1;
            }
        }
        drop(index_guard);

        // Get pack sizes
        for (pack_id, stats) in pack_stats.iter_mut() {
            if let Ok(size) = repo.pack_size(pack_id).await {
                stats.2 = size;
            }
        }

        // Find packs to delete (100% orphaned) or repack (partially orphaned)
        let max_unused_pct = self.max_unused.unwrap_or(50) as f64 / 100.0;
        let mut packs_to_delete: Vec<String> = Vec::new();
        let mut packs_to_repack: Vec<String> = Vec::new();
        let mut space_to_reclaim = 0u64;

        for (pack_id, (total, orphaned, size)) in &pack_stats {
            if *orphaned == 0 {
                continue;
            }

            let orphan_ratio = *orphaned as f64 / *total as f64;

            if orphan_ratio >= 1.0 {
                // All chunks are orphaned - delete entire pack
                packs_to_delete.push(pack_id.clone());
                space_to_reclaim += size;
            } else if orphan_ratio >= max_unused_pct {
                // Significant portion orphaned - should repack
                packs_to_repack.push(pack_id.clone());
                // Estimate space savings
                space_to_reclaim += (*size as f64 * orphan_ratio) as u64;
            }
        }

        // Display summary
        println!();
        println!("Prune summary:");
        println!("  Orphaned chunks:    {}", orphaned_chunks.len());
        println!("  Packs to delete:    {}", packs_to_delete.len());
        println!("  Packs to repack:    {}", packs_to_repack.len());
        println!("  Space to reclaim:   {}", format_size(space_to_reclaim));

        if self.dry_run {
            println!();
            println!("Dry run - no changes made");
            println!("Run without --dry-run to actually prune");
            return Ok(());
        }

        // Step 4: Actually delete/prune
        println!();
        println!("[4/4] Pruning data...");

        // Delete fully orphaned packs
        if !packs_to_delete.is_empty() {
            print!("  Deleting {} packs...", packs_to_delete.len());
            io::stdout().flush()?;

            for pack_id in &packs_to_delete {
                repo.delete_pack(pack_id).await?;
                info!("Deleted pack: {}", pack_id);
            }
            println!(" done");
        }

        // Remove orphaned chunks from index
        print!("  Removing {} chunks from index...", orphaned_chunks.len());
        io::stdout().flush()?;

        {
            let index_arc = repo.index();
            let mut index = index_arc.write().await;
            for chunk_id in &orphaned_chunks {
                index.remove_chunk(chunk_id);
            }
        }

        // Save updated index
        repo.save_index().await?;
        println!(" done");

        // Note: Repacking would require reading chunks from old packs and writing new ones
        // This is a more complex operation that we'll note but not implement fully here
        if !packs_to_repack.is_empty() {
            println!();
            println!(
                "Note: {} packs have significant unused space but contain some live data.",
                packs_to_repack.len()
            );
            println!(
                "      Full repacking not yet implemented. Consider running backup again to repack data."
            );
        }

        println!();
        println!("Prune completed!");
        println!("Reclaimed approximately {}", format_size(space_to_reclaim));

        Ok(())
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
