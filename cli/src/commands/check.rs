use anyhow::{Result, anyhow};
use clap::Args;
use ghostsnap_core::Repository;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::io::{self, Write};
use tracing::warn;

#[derive(Args)]
pub struct CheckCommand {
    #[arg(long, help = "Read and verify all data (slow but thorough)")]
    read_data: bool,

    #[arg(long, help = "Check specific snapshot only")]
    snapshot: Option<String>,
}

impl CheckCommand {
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

        println!("Checking repository integrity...");
        println!();

        let mut errors = 0;
        let mut warnings = 0;

        // 1. Check all snapshots
        let snapshots = if let Some(ref id) = self.snapshot {
            vec![id.clone()]
        } else {
            repo.list_snapshots().await?
        };

        println!("[1/5] Checking {} snapshots...", snapshots.len());
        let pb = ProgressBar::new(snapshots.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{bar:40} {pos}/{len} snapshots")
                .unwrap(),
        );

        let mut all_tree_ids = HashSet::new();
        let mut all_chunk_ids = HashSet::new();

        for snapshot_id in &snapshots {
            match repo.load_snapshot(snapshot_id).await {
                Ok(snapshot) => {
                    all_tree_ids.insert(snapshot.tree);

                    // Load tree and collect chunk IDs
                    match repo.load_tree(&snapshot.tree).await {
                        Ok(tree) => {
                            for node in &tree.nodes {
                                for chunk_ref in &node.chunks {
                                    all_chunk_ids.insert(chunk_ref.id);
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Cannot load tree {} for snapshot {}: {}",
                                snapshot.tree.short_string(),
                                snapshot_id,
                                e
                            );
                            errors += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!("Cannot load snapshot {}: {}", snapshot_id, e);
                    errors += 1;
                }
            }
            pb.inc(1);
        }
        pb.finish_and_clear();
        println!(
            "  Snapshots: {} checked, {} errors",
            snapshots.len(),
            errors
        );

        // 2. Check tree objects
        println!("[2/5] Checking {} tree objects...", all_tree_ids.len());
        let tree_errors_before = errors;
        for tree_id in &all_tree_ids {
            if let Err(e) = repo.load_tree(tree_id).await {
                warn!("Cannot load tree {}: {}", tree_id.short_string(), e);
                errors += 1;
            }
        }
        println!(
            "  Trees: {} checked, {} errors",
            all_tree_ids.len(),
            errors - tree_errors_before
        );

        // 3. Check chunk index consistency
        println!("[3/5] Checking {} chunk references...", all_chunk_ids.len());
        let pb = ProgressBar::new(all_chunk_ids.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{bar:40} {pos}/{len} chunks")
                .unwrap(),
        );

        let mut missing_chunks = 0;
        let index = repo.index();
        let index_guard = index.read().await;

        for chunk_id in &all_chunk_ids {
            if !index_guard.has_chunk(chunk_id) {
                warn!(
                    "Chunk {} referenced but not in index",
                    chunk_id.short_string()
                );
                missing_chunks += 1;
            }
            pb.inc(1);
        }
        drop(index_guard);
        pb.finish_and_clear();

        if missing_chunks > 0 {
            errors += missing_chunks;
            println!(
                "  Chunks: {} referenced, {} missing from index",
                all_chunk_ids.len(),
                missing_chunks
            );
        } else {
            println!(
                "  Chunks: {} referenced, all present in index",
                all_chunk_ids.len()
            );
        }

        // 4. Check pack files
        let packs = repo.list_packs().await?;
        let existing_packs: HashSet<_> = packs.iter().cloned().collect();

        // 4a. Verify index pack references point to existing packs
        println!("[4/5] Verifying index pack references...");
        let index = repo.index();
        let index_guard = index.read().await;
        let mut referenced_packs: HashSet<String> = HashSet::new();
        for (_, location) in index_guard.iter_chunks() {
            referenced_packs.insert(location.pack_id.clone());
        }
        drop(index_guard);

        let missing_packs: Vec<_> = referenced_packs
            .difference(&existing_packs)
            .collect();

        if !missing_packs.is_empty() {
            for pack_id in &missing_packs {
                warn!("Pack {} referenced in index but does not exist", pack_id);
            }
            errors += missing_packs.len();
            println!(
                "  Index references: {} packs, {} missing",
                referenced_packs.len(),
                missing_packs.len()
            );
        } else {
            println!(
                "  Index references: {} packs, all present",
                referenced_packs.len()
            );
        }

        // 4b. Check pack file integrity
        println!("[5/5] Checking {} pack files...", packs.len());

        if self.read_data {
            let pb = ProgressBar::new(packs.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{bar:40} {pos}/{len} packs")
                    .unwrap(),
            );

            let mut pack_errors = 0;
            for pack_id in &packs {
                match repo.load_pack(pack_id).await {
                    Ok(_pack) => {
                        // Pack loaded successfully (decrypted and deserialized)
                    }
                    Err(e) => {
                        warn!("Cannot load pack {}: {}", pack_id, e);
                        pack_errors += 1;
                    }
                }
                pb.inc(1);
            }
            pb.finish_and_clear();
            errors += pack_errors;
            println!(
                "  Packs: {} checked (read all data), {} errors",
                packs.len(),
                pack_errors
            );
        } else {
            // Just check pack files exist
            let mut pack_errors = 0;
            for pack_id in &packs {
                if !repo.pack_exists(pack_id).await? {
                    warn!("Pack file missing: {}", pack_id);
                    pack_errors += 1;
                }
            }
            errors += pack_errors;
            println!(
                "  Packs: {} exist, {} missing (use --read-data for full verification)",
                packs.len(),
                pack_errors
            );
        }

        // Check for orphaned data (chunks in index but not referenced)
        let index = repo.index();
        let index_guard = index.read().await;
        let indexed_chunks: HashSet<_> = index_guard.iter_chunks().map(|(id, _)| *id).collect();
        drop(index_guard);

        let orphaned: Vec<_> = indexed_chunks.difference(&all_chunk_ids).collect();
        if !orphaned.is_empty() {
            warnings += 1;
            println!();
            println!(
                "Warning: {} orphaned chunks found (not referenced by any snapshot)",
                orphaned.len()
            );
            println!("  Run 'ghostsnap prune' to reclaim space");
        }

        // Summary
        println!();
        if errors == 0 && warnings == 0 {
            println!("Repository is healthy!");
        } else {
            if errors > 0 {
                println!("Found {} errors", errors);
            }
            if warnings > 0 {
                println!("Found {} warnings", warnings);
            }
        }

        if errors > 0 {
            Err(anyhow!("Repository check failed with {} errors", errors))
        } else {
            Ok(())
        }
    }
}
