use anyhow::{Result, anyhow};
use clap::Args;
use ghostsnap_core::{LockManager, LockType, Repository};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::io::{self, Write};
use tracing::{debug, info};

#[derive(Args)]
pub struct CopyCommand {
    #[arg(help = "Snapshot ID to copy (full or short prefix)")]
    snapshot_id: String,

    #[arg(long, help = "Destination repository path")]
    repo2: String,

    #[arg(long, help = "Password for destination repository")]
    password2: Option<String>,

    #[arg(long, short = 'n', help = "Dry run - don't actually copy")]
    dry_run: bool,
}

impl CopyCommand {
    pub async fn run(&self, cli: &crate::Cli) -> Result<()> {
        let src_repo_location = crate::commands::parse_repository_location(cli.repo.as_ref())?;
        let src_repo_display = src_repo_location.display();
        let dst_repo_location = ghostsnap_core::storage::RepositoryLocation::parse(&self.repo2)
            .map_err(|e| anyhow!(e.to_string()))?;
        let dst_repo_display = dst_repo_location.display();

        let src_password = cli
            .password
            .clone()
            .or_else(|| {
                print!("Enter source repository password: ");
                io::stdout().flush().ok()?;
                rpassword::read_password().ok()
            })
            .ok_or_else(|| anyhow!("Source password required"))?;

        let dst_password = self
            .password2
            .clone()
            .or_else(|| {
                print!("Enter destination repository password: ");
                io::stdout().flush().ok()?;
                rpassword::read_password().ok()
            })
            .ok_or_else(|| anyhow!("Destination password required"))?;

        // Open source repository
        info!("Opening source repository: {}", src_repo_display);
        let src_repo = Repository::open_at_location(src_repo_location, &src_password).await?;

        // Open destination repository
        info!("Opening destination repository: {}", dst_repo_display);
        let dst_repo = Repository::open_at_location(dst_repo_location, &dst_password).await?;

        // Acquire exclusive lock on destination repository only (source is read-only)
        let _dst_lock = if let Some(repo_path) = dst_repo.local_path() {
            let lock_manager = LockManager::new(repo_path);
            Some(lock_manager.acquire(LockType::Exclusive, "copy").await?)
        } else {
            tracing::warn!("Repository locking not supported for remote destination repository");
            None
        };

        // Resolve snapshot ID
        let full_snapshot_id = self
            .resolve_snapshot_id(&src_repo, &self.snapshot_id)
            .await?;

        // Load snapshot and tree
        let snapshot = src_repo.load_snapshot(&full_snapshot_id).await?;
        let tree = src_repo.load_tree(&snapshot.tree).await?;

        println!(
            "Copying snapshot {} from {} to {}",
            &full_snapshot_id[..8],
            src_repo_display,
            dst_repo_display
        );
        println!("  Created: {}", snapshot.time.format("%Y-%m-%d %H:%M:%S"));
        println!("  Host: {}", snapshot.hostname);
        println!("  Files: {}", tree.nodes.len());

        if self.dry_run {
            println!();
            println!("Dry run - no data will be copied");
        }

        // Collect all chunks needed
        let mut chunks_needed: HashSet<_> = HashSet::new();
        for node in &tree.nodes {
            for chunk_ref in &node.chunks {
                chunks_needed.insert(chunk_ref.id);
            }
        }

        println!();
        println!("Analyzing {} chunks...", chunks_needed.len());

        // Check which chunks already exist in destination
        let mut chunks_to_copy = Vec::new();
        for chunk_id in &chunks_needed {
            if !dst_repo.has_chunk(chunk_id).await? {
                chunks_to_copy.push(*chunk_id);
            }
        }

        println!(
            "  {} chunks need to be copied ({} already exist)",
            chunks_to_copy.len(),
            chunks_needed.len() - chunks_to_copy.len()
        );

        if self.dry_run {
            println!();
            println!(
                "Dry run completed - would copy {} chunks",
                chunks_to_copy.len()
            );
            return Ok(());
        }

        // Copy chunks
        if !chunks_to_copy.is_empty() {
            println!();
            println!("Copying {} chunks...", chunks_to_copy.len());

            let pb = ProgressBar::new(chunks_to_copy.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{bar:40} {pos}/{len} chunks")
                    .unwrap(),
            );

            // Group chunks by source pack for efficient reading
            // For simplicity, we'll copy chunks individually (could be optimized)
            use ghostsnap_core::pack::PackManager;
            let mut pack_manager = PackManager::new(64 * 1024 * 1024);

            for chunk_id in &chunks_to_copy {
                // Load chunk from source
                let chunk_data = src_repo.load_chunk(chunk_id).await?;

                // Add to pack manager for destination
                if let Some(finished_pack) = pack_manager.add_chunk(*chunk_id, &chunk_data)? {
                    // Save pack to destination
                    dst_repo.save_pack(&finished_pack).await?;

                    // Index chunks in destination
                    for (cid, entry) in &finished_pack.chunks {
                        dst_repo
                            .save_chunk_location(
                                cid,
                                &finished_pack.header.pack_id,
                                entry.offset,
                                entry.length,
                            )
                            .await?;
                    }
                }

                pb.inc(1);
            }

            // Save final pack
            if let Some(finished_pack) = pack_manager.finish_current_pack() {
                dst_repo.save_pack(&finished_pack).await?;

                for (cid, entry) in &finished_pack.chunks {
                    dst_repo
                        .save_chunk_location(
                            cid,
                            &finished_pack.header.pack_id,
                            entry.offset,
                            entry.length,
                        )
                        .await?;
                }
            }

            pb.finish_and_clear();
            println!("  Chunks copied successfully");
        }

        // Save tree to destination
        println!("Saving tree...");
        let dst_tree_id = dst_repo.save_tree(&tree).await?;
        debug!("Saved tree with ID: {}", dst_tree_id.to_hex());

        // Create new snapshot in destination (with same metadata but new tree reference)
        let mut dst_snapshot = snapshot.clone();
        // Tree ID should match since tree content is the same
        // But we saved the tree to destination, so use that ID
        dst_snapshot.tree = dst_tree_id;
        // Clear parent reference - parent snapshot does not exist in destination repository
        dst_snapshot.parent = None;

        println!("Saving snapshot...");
        dst_repo.save_snapshot(&dst_snapshot).await?;

        // Save destination index
        dst_repo.save_index().await?;

        println!();
        println!("Copy completed!");
        println!(
            "  Snapshot {} is now available in {}",
            &full_snapshot_id[..8],
            dst_repo_display
        );

        Ok(())
    }

    async fn resolve_snapshot_id(&self, repo: &Repository, snapshot_id: &str) -> Result<String> {
        if snapshot_id.len() >= 36 {
            return Ok(snapshot_id.to_string());
        }

        let all_snapshots = repo.list_snapshots().await?;
        let matches: Vec<_> = all_snapshots
            .iter()
            .filter(|id| id.starts_with(snapshot_id))
            .collect();

        match matches.len() {
            0 => Err(anyhow!(
                "No snapshot found with ID starting with '{}'",
                snapshot_id
            )),
            1 => Ok(matches[0].clone()),
            _ => Err(anyhow!(
                "Ambiguous snapshot ID '{}' - matches {} snapshots",
                snapshot_id,
                matches.len()
            )),
        }
    }
}
