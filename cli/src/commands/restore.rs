use anyhow::{anyhow, Result};
use ghostsnap_core::{Repository, SnapshotID};
use ghostsnap_core::snapshot::{Snapshot, Tree};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::io::{self, Write};
use tracing::{info, debug, warn};
use tokio::fs;

pub struct RestoreCommand;

impl RestoreCommand {
    pub async fn run(
        snapshot_id: String,
        target: String,
        paths: Vec<String>,
        cli: &crate::Cli
    ) -> Result<()> {
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

        info!("Loading snapshot: {}", snapshot_id);
        let snapshot = repo.load_snapshot(&snapshot_id).await?;

        let target_path = PathBuf::from(target);
        if !target_path.exists() {
            fs::create_dir_all(&target_path).await?;
        }

        println!("📸 Restoring snapshot: {}", snapshot.short_id());
        println!("📅 Created: {}", snapshot.time.format("%Y-%m-%d %H:%M:%S UTC"));
        println!("🖥️  Host: {}", snapshot.hostname);
        println!("👤 User: {}", snapshot.username);
        println!("📂 Target: {}", target_path.display());

        // Load the tree
        let tree = repo.load_tree(&snapshot.tree).await?;

        // Filter nodes to restore
        let nodes_to_restore = if paths.is_empty() {
            tree.nodes.clone()
        } else {
            tree.nodes.into_iter()
                .filter(|node| paths.iter().any(|p| node.name.contains(p)))
                .collect()
        };

        if nodes_to_restore.is_empty() {
            println!("No files to restore");
            return Ok(());
        }

        println!("📁 Restoring {} files...", nodes_to_restore.len());

        let pb = ProgressBar::new(nodes_to_restore.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
                .unwrap(),
        );

        let mut restored_count = 0;
        let mut failed_count = 0;

        for node in nodes_to_restore {
            pb.set_message(format!("Restoring {}", node.name));
            
            match Self::restore_file(&repo, &node, &target_path).await {
                Ok(_) => {
                    restored_count += 1;
                    debug!("Successfully restored: {}", node.name);
                }
                Err(e) => {
                    failed_count += 1;
                    warn!("Failed to restore {}: {}", node.name, e);
                }
            }

            pb.inc(1);
        }

        pb.finish_with_message("Restore completed");

        println!("✅ Restore completed!");
        println!("📁 Restored: {}", restored_count);
        if failed_count > 0 {
            println!("❌ Failed: {}", failed_count);
        }
        println!("📂 Location: {}", target_path.display());

        Ok(())
    }

    async fn restore_file(
        repo: &Repository,
        node: &ghostsnap_core::TreeNode,
        target_base: &Path,
    ) -> Result<()> {
        if !node.is_file() {
            return Ok(()); // Skip non-files for now
        }

        let file_path = target_base.join(&node.name);
        
        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Reconstruct file from chunks
        let mut file_data = Vec::new();
        
        for chunk_ref in &node.chunks {
            let chunk_data = repo.load_chunk(&chunk_ref.id).await?;
            file_data.extend_from_slice(&chunk_data);
        }

        // Write file
        fs::write(&file_path, file_data).await?;

        // Set permissions if on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(node.mode);
            fs::set_permissions(&file_path, permissions).await?;
        }

        debug!("Restored: {} ({} bytes)", file_path.display(), node.size);
        Ok(())
    }
}