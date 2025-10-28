use anyhow::{anyhow, Result};
use clap::Args;
use ghostsnap_core::{Repository, chunker::Chunker, types::TreeNode, NodeType};
use ghostsnap_core::snapshot::{Snapshot, Tree};
use ghostsnap_core::pack::PackFile;
use ghostsnap_core::pack::PackManager;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::io::{self, Write};
use tracing::{info, debug, warn};
use walkdir::WalkDir;
use tokio::fs;

#[derive(Args)]
pub struct BackupCommand {
    #[arg(help = "Paths to backup")]
    paths: Vec<String>,
    
    #[arg(long, help = "Backup tags")]
    tag: Vec<String>,
    
    #[arg(long, help = "Exclude patterns")]
    exclude: Vec<String>,
    
    #[arg(long, help = "Exclude if file present")]
    exclude_if_present: Vec<String>,
    
    #[arg(long, help = "Stay on same filesystem")]
    one_file_system: bool,
    
    #[arg(long, help = "Dry run - don't actually backup")]
    dry_run: bool,
    
    #[arg(long, help = "Parent snapshot ID")]
    parent: Option<String>,
    
    #[arg(long, help = "Hostname override")]
    hostname: Option<String>,
}

impl BackupCommand {
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
        
        if self.paths.is_empty() {
            return Err(anyhow!("At least one path must be specified"));
        }
        
        let paths: Vec<PathBuf> = self.paths.iter().map(PathBuf::from).collect();
        
        info!("Starting backup of {} paths", paths.len());
        
        if self.dry_run {
            println!("DRY RUN - no data will be written");
        }
        
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );
        pb.set_message("Scanning files...");
        
        let mut total_files = 0;
        let mut total_size = 0u64;
        let mut file_list = Vec::new(); // Store (PathBuf, TreeNode) pairs

        for path in &paths {
            if !path.exists() {
                return Err(anyhow!("Path does not exist: {}", path.display()));
            }

            for entry in WalkDir::new(path)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if self.should_exclude(&entry.path()) {
                    continue;
                }

                let metadata = entry.metadata()?;
                let file_path = entry.path().to_path_buf();

                if metadata.is_file() {
                    total_files += 1;
                    total_size += metadata.len();

                    let relative_path = file_path.strip_prefix(path)
                        .unwrap_or(&file_path);

                    debug!("Found file: {}", relative_path.display());

                    #[cfg(unix)]
                    let mode = {
                        use std::os::unix::fs::PermissionsExt;
                        metadata.permissions().mode()
                    };
                    #[cfg(not(unix))]
                    let mode = 0o644;

                    let node = TreeNode {
                        name: relative_path.to_string_lossy().to_string(),
                        node_type: NodeType::File,
                        mode,
                        uid: 0,       // Will be properly set in future
                        gid: 0,       // Will be properly set in future
                        size: metadata.len(),
                        mtime: metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64,
                        subtree_id: None,
                        chunks: Vec::new(), // Will be filled during actual backup
                    };

                    file_list.push((file_path, node));
                }
            }
        }
        
        pb.finish_with_message(format!("Found {} files ({:.2} MB)", 
            total_files, 
            total_size as f64 / 1024.0 / 1024.0
        ));
        
        if !self.dry_run {
            println!("Backing up {} files...", total_files);
            
            let chunker = Chunker::default();
            let mut pack_manager = PackManager::new(64 * 1024 * 1024); // 64MB pack size
            let mut processed_nodes = Vec::new();
            
            // Progress bar for backup
            let backup_pb = ProgressBar::new(total_files);
            backup_pb.set_style(
                ProgressStyle::default_bar()
                    .template("{bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
                    .unwrap(),
            );
            
            for (i, (file_path, mut node)) in file_list.into_iter().enumerate() {
                backup_pb.set_message(format!("Processing {}", node.name));

                match self.process_file(&repo, &chunker, &mut pack_manager, &file_path).await {
                    Ok(chunks) => {
                        node.chunks = chunks;
                        let node_name = node.name.clone();
                        processed_nodes.push(node);
                        debug!("Successfully processed: {}", node_name);
                    }
                    Err(e) => {
                        warn!("Failed to process {}: {}", node.name, e);
                        // Continue with other files
                    }
                }

                backup_pb.inc(1);

                // Periodically save completed packs
                if i % 100 == 0 {
                    if let Some(pack) = pack_manager.finish_current_pack() {
                        if let Err(e) = self.save_pack_and_index(&repo, &pack).await {
                            warn!("Failed to save pack: {}", e);
                        }
                    }
                }
            }
            
            // Save final pack
            if let Some(pack) = pack_manager.finish_current_pack() {
                if let Err(e) = self.save_pack_and_index(&repo, &pack).await {
                    warn!("Failed to save final pack: {}", e);
                }
            }
            
            backup_pb.finish_with_message("Files processed");
            
            // Create and save tree
            let mut tree = Tree::new();
            for node in processed_nodes {
                tree.add_node(node);
            }
            
            let tree_id = repo.save_tree(&tree).await?;
            
            // Create snapshot
            let mut snapshot = Snapshot::new(paths.clone(), tree_id);
            if let Some(parent_id) = &self.parent {
                snapshot = snapshot.with_parent(parent_id.clone());
            }
            snapshot = snapshot.with_tags(self.tag.clone());
            snapshot = snapshot.with_excludes(self.exclude.clone());
            
            if let Some(hostname) = &self.hostname {
                // Would need to add setter for hostname override
                // For now, use the default hostname from Snapshot::new
            }
            
            // Save snapshot
            repo.save_snapshot(&snapshot).await?;
            
            println!("✅ Backup completed successfully!");
            println!("📸 Snapshot: {}", snapshot.short_id());
            println!("📁 Files: {}", total_files);
            println!("💾 Size: {:.2} MB", total_size as f64 / 1024.0 / 1024.0);
            println!("🌳 Tree: {}", tree_id.short_string());
            
        } else {
            println!("Dry run completed - would backup {} files ({:.2} MB)", 
                total_files, 
                total_size as f64 / 1024.0 / 1024.0
            );
        }
        
        Ok(())
    }
    
    fn should_exclude(&self, _path: &std::path::Path) -> bool {
        // TODO: Implement pattern matching for excludes
        false
    }

    async fn process_file(
        &self,
        repo: &Repository,
        chunker: &Chunker,
        pack_manager: &mut PackManager,
        file_path: &PathBuf,
    ) -> Result<Vec<ghostsnap_core::ChunkRef>> {
        let file_data = fs::read(file_path).await?;
        let chunks = chunker.chunk_data(&file_data);
        let mut chunk_refs = Vec::new();

        for chunk in chunks {
            let chunk_id = chunk.id();

            // Check if chunk already exists (deduplication)
            if !repo.has_chunk(&chunk_id).await? {
                // Add chunk to pack (chunk_id is Copy, so this is cheap)
                if let Some(finished_pack) = pack_manager.add_chunk(chunk_id, chunk.data())? {
                    // Save the completed pack
                    self.save_pack_and_index(repo, &finished_pack).await?;
                }
            }

            // Create chunk reference
            chunk_refs.push(ghostsnap_core::ChunkRef {
                id: chunk_id,
                offset: 0, // Will be updated when pack is saved
                length: chunk.data().len() as u32,
            });
        }

        Ok(chunk_refs)
    }

    async fn save_pack_and_index(
        &self,
        repo: &Repository,
        pack: &PackFile,
    ) -> Result<()> {
        // Save the pack file
        repo.save_pack(pack).await?;

        // Index all chunks in the pack
        for (chunk_id, chunk_entry) in &pack.chunks {
            repo.save_chunk_location(
                chunk_id,
                &pack.header.pack_id,
                chunk_entry.offset,
                chunk_entry.length,
            ).await?;
        }

        info!("Saved pack: {} with {} chunks", pack.header.pack_id, pack.chunks.len());
        Ok(())
    }
}