use anyhow::{Result, anyhow};
use clap::Args;
use ghostsnap_core::pack::PackFile;
use ghostsnap_core::pack::PackManager;
use ghostsnap_core::snapshot::{Snapshot, Tree};
use ghostsnap_core::{LockManager, LockType, NodeType, Repository, chunker::Chunker, types::TreeNode};
use globset::{Glob, GlobSet, GlobSetBuilder};
use indicatif::{HumanBytes, HumanDuration, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

#[derive(Args)]
pub struct BackupCommand {
    #[arg(help = "Paths to backup")]
    paths: Vec<String>,

    #[arg(long, help = "Backup tags")]
    tag: Vec<String>,

    #[arg(long, short = 'e', help = "Exclude patterns (glob syntax)")]
    exclude: Vec<String>,

    #[arg(long, help = "Exclude if file present in directory")]
    exclude_if_present: Vec<String>,

    #[arg(long, short = 'x', help = "Stay on same filesystem")]
    one_file_system: bool,

    #[arg(long, short = 'n', help = "Dry run - don't actually backup")]
    dry_run: bool,

    #[arg(long, help = "Parent snapshot ID for incremental backup")]
    parent: Option<String>,

    #[arg(long, help = "Hostname override")]
    hostname: Option<String>,

    #[arg(long, help = "Don't backup extended attributes")]
    no_xattr: bool,

    #[arg(
        long,
        help = "Maximum file size to backup (e.g., 1G, 500M). Files larger than this are skipped"
    )]
    max_file_size: Option<String>,

    #[arg(long, help = "Don't detect and preserve hardlinks")]
    no_hardlinks: bool,
}

impl BackupCommand {
    /// Parses a human-readable size string (e.g., "1G", "500M", "100K") into bytes.
    fn parse_size(&self, size_str: &str) -> Result<u64> {
        let size_str = size_str.trim().to_uppercase();
        let (num_str, multiplier) = if size_str.ends_with("G") || size_str.ends_with("GB") {
            (
                size_str.trim_end_matches("GB").trim_end_matches("G"),
                1024 * 1024 * 1024,
            )
        } else if size_str.ends_with("M") || size_str.ends_with("MB") {
            (
                size_str.trim_end_matches("MB").trim_end_matches("M"),
                1024 * 1024,
            )
        } else if size_str.ends_with("K") || size_str.ends_with("KB") {
            (size_str.trim_end_matches("KB").trim_end_matches("K"), 1024)
        } else {
            (size_str.as_str(), 1)
        };

        let num: u64 = num_str
            .parse()
            .map_err(|_| anyhow!("Invalid size format: {}", size_str))?;
        Ok(num * multiplier)
    }

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

        // Parse max file size if provided
        let max_file_size = match &self.max_file_size {
            Some(size_str) => Some(self.parse_size(size_str)?),
            None => None,
        };

        info!("Opening repository at: {}", repo_location.display());
        let repo = Repository::open_at_location(repo_location, &password).await?;

        // Acquire exclusive lock for backup operation
        let _lock = if let Some(repo_path) = repo.local_path() {
            let lock_manager = LockManager::new(repo_path);
            Some(lock_manager.acquire(LockType::Exclusive, "backup").await?)
        } else {
            tracing::warn!("Repository locking not supported for remote repositories");
            None
        };

        if self.paths.is_empty() {
            return Err(anyhow!("At least one path must be specified"));
        }

        let paths: Vec<PathBuf> = self.paths.iter().map(PathBuf::from).collect();

        // Build exclude pattern matcher
        let excludes = self.build_exclude_matcher()?;

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

        let mut total_files = 0u64;
        let mut total_dirs = 0u64;
        let mut total_symlinks = 0u64;
        let mut total_hardlinks = 0u64;
        let mut total_size = 0u64;
        let mut skipped_large = 0u64;
        let mut file_list = Vec::new();

        // Track inodes for hardlink detection (inode -> first relative path seen)
        #[cfg(unix)]
        let mut inode_map: HashMap<(u64, u64), String> = HashMap::new(); // (dev, inode) -> path

        for path in &paths {
            if !path.exists() {
                return Err(anyhow!("Path does not exist: {}", path.display()));
            }

            let mut walker = WalkDir::new(path).follow_links(false);
            if self.one_file_system {
                walker = walker.same_file_system(true);
            }
            for entry in walker.into_iter().filter_map(|e| e.ok())
            {
                let entry_path = entry.path();

                // Check exclude patterns
                if self.should_exclude(entry_path, &excludes) {
                    debug!("Excluding: {}", entry_path.display());
                    continue;
                }

                // Check exclude-if-present
                if self.check_exclude_if_present(entry_path) {
                    debug!("Excluding (marker file present): {}", entry_path.display());
                    continue;
                }

                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Cannot read metadata for {}: {}", entry_path.display(), e);
                        continue;
                    }
                };

                let relative_path = entry_path.strip_prefix(path).unwrap_or(entry_path);

                // Get Unix-specific metadata including inode
                #[cfg(unix)]
                let (mode, uid, gid, inode, nlink, dev) = {
                    use std::os::unix::fs::MetadataExt;
                    (
                        metadata.mode(),
                        metadata.uid(),
                        metadata.gid(),
                        metadata.ino(),
                        metadata.nlink() as u32,
                        metadata.dev(),
                    )
                };
                #[cfg(not(unix))]
                let (mode, uid, gid, inode, nlink, dev) = {
                    (
                        if metadata.is_dir() { 0o755 } else { 0o644 },
                        0u32,
                        0u32,
                        0u64,
                        1u32,
                        0u64,
                    )
                };

                let mtime = metadata
                    .modified()
                    .map(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0)
                    })
                    .unwrap_or(0);

                // Capture extended attributes if enabled
                let xattr = if !self.no_xattr {
                    read_xattrs(entry_path)
                } else {
                    None
                };

                if metadata.is_file() {
                    // Check max file size
                    if let Some(max_size) = max_file_size
                        && metadata.len() > max_size
                    {
                        warn!(
                            "Skipping large file {} ({} > {})",
                            entry_path.display(),
                            HumanBytes(metadata.len()),
                            HumanBytes(max_size)
                        );
                        skipped_large += 1;
                        continue;
                    }

                    total_files += 1;
                    total_size += metadata.len();

                    // Detect sparse file holes
                    let sparse_holes = detect_sparse_holes(entry_path, metadata.len());

                    // Check for hardlinks
                    #[cfg(unix)]
                    let (is_hardlink, hardlink_target) = if !self.no_hardlinks && nlink > 1 {
                        let inode_key = (dev, inode);
                        if let Some(first_path) = inode_map.get(&inode_key) {
                            // This is a subsequent hardlink to an already-seen file
                            total_hardlinks += 1;
                            (true, Some(first_path.clone()))
                        } else {
                            // First occurrence of this inode
                            inode_map
                                .insert(inode_key, relative_path.to_string_lossy().to_string());
                            (false, None)
                        }
                    } else {
                        (false, None)
                    };

                    #[cfg(not(unix))]
                    let (is_hardlink, hardlink_target): (bool, Option<String>) = (false, None);

                    let node = TreeNode {
                        name: relative_path.to_string_lossy().to_string(),
                        node_type: NodeType::File,
                        mode,
                        uid,
                        gid,
                        size: metadata.len(),
                        mtime,
                        link_target: None,
                        subtree_id: None,
                        chunks: Vec::new(),
                        xattr: xattr.clone(),
                        sparse_holes,
                        inode: if !self.no_hardlinks && nlink > 1 {
                            Some(inode)
                        } else {
                            None
                        },
                        nlink: if !self.no_hardlinks && nlink > 1 {
                            Some(nlink)
                        } else {
                            None
                        },
                        hardlink_target,
                    };

                    file_list.push((entry_path.to_path_buf(), node, is_hardlink));
                } else if metadata.is_dir() {
                    total_dirs += 1;

                    let node = TreeNode {
                        name: relative_path.to_string_lossy().to_string(),
                        node_type: NodeType::Directory,
                        mode,
                        uid,
                        gid,
                        size: 0,
                        mtime,
                        link_target: None,
                        subtree_id: None,
                        chunks: Vec::new(),
                        xattr: xattr.clone(),
                        sparse_holes: None,
                        inode: None,
                        nlink: None,
                        hardlink_target: None,
                    };

                    file_list.push((entry_path.to_path_buf(), node, false));
                } else if metadata.is_symlink() {
                    total_symlinks += 1;

                    // Read symlink target
                    let link_target = match std::fs::read_link(entry_path) {
                        Ok(target) => Some(target.to_string_lossy().to_string()),
                        Err(e) => {
                            warn!(
                                "Cannot read symlink target for {}: {}",
                                entry_path.display(),
                                e
                            );
                            None
                        }
                    };

                    let node = TreeNode {
                        name: relative_path.to_string_lossy().to_string(),
                        node_type: NodeType::Symlink,
                        mode,
                        uid,
                        gid,
                        size: 0,
                        mtime,
                        link_target,
                        subtree_id: None,
                        chunks: Vec::new(),
                        xattr,
                        sparse_holes: None,
                        inode: None,
                        nlink: None,
                        hardlink_target: None,
                    };

                    file_list.push((entry_path.to_path_buf(), node, false));
                }
            }
        }

        let mut scan_summary = format!(
            "Found {} files, {} dirs, {} symlinks",
            total_files, total_dirs, total_symlinks
        );
        if total_hardlinks > 0 {
            scan_summary.push_str(&format!(", {} hardlinks", total_hardlinks));
        }
        if skipped_large > 0 {
            scan_summary.push_str(&format!(", {} skipped (too large)", skipped_large));
        }
        scan_summary.push_str(&format!(" ({})", HumanBytes(total_size)));

        pb.finish_with_message(scan_summary);

        if !self.dry_run {
            println!("Backing up {} items...", file_list.len());

            let chunker = Chunker::new_default();
            let mut pack_manager = PackManager::new(64 * 1024 * 1024);
            let mut processed_nodes = Vec::new();

            let backup_pb = ProgressBar::new(total_size);
            backup_pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta}) {msg}")
                    .unwrap()
                    .progress_chars("#>-"),
            );

            let start_time = Instant::now();
            let mut bytes_processed = 0u64;
            let mut new_chunks = 0u64;
            let mut dedup_chunks = 0u64;
            let mut failed_files = 0u64;

            for (i, (file_path, mut node, is_hardlink)) in file_list.into_iter().enumerate() {
                backup_pb.set_message(node.name.clone());

                // Only process files for chunking (skip hardlinks - they reference the original)
                if node.node_type == NodeType::File && !is_hardlink {
                    match self
                        .process_file_with_stats(&repo, &chunker, &mut pack_manager, &file_path)
                        .await
                    {
                        Ok((chunks, new, dedup)) => {
                            node.chunks = chunks;
                            new_chunks += new;
                            dedup_chunks += dedup;
                            debug!("Successfully processed: {}", node.name);
                        }
                        Err(e) => {
                            warn!("Failed to process {}: {}", node.name, e);
                            failed_files += 1;
                            bytes_processed += node.size;
                            backup_pb.set_position(bytes_processed);
                            continue; // Skip this node - don't save broken entry
                        }
                    }
                    bytes_processed += node.size;
                    backup_pb.set_position(bytes_processed);
                } else if is_hardlink {
                    // Hardlinks don't need chunk processing - they'll reference the original
                    debug!(
                        "Hardlink detected: {} -> {:?}",
                        node.name, node.hardlink_target
                    );
                    bytes_processed += node.size;
                    backup_pb.set_position(bytes_processed);
                }

                processed_nodes.push(node);

                // Periodically save completed packs
                if i % 100 == 0
                    && let Some(pack) = pack_manager.finish_current_pack()
                    && let Err(e) = self.save_pack_and_index(&repo, &pack).await
                {
                    warn!("Failed to save pack: {}", e);
                }
            }

            // Save final pack
            if let Some(pack) = pack_manager.finish_current_pack()
                && let Err(e) = self.save_pack_and_index(&repo, &pack).await
            {
                warn!("Failed to save final pack: {}", e);
            }

            let elapsed = start_time.elapsed();
            let throughput = if elapsed.as_secs() > 0 {
                bytes_processed / elapsed.as_secs()
            } else {
                bytes_processed
            };

            backup_pb.finish_with_message(format!(
                "Done ({} new, {} dedup, {} @ {}/s)",
                new_chunks,
                dedup_chunks,
                HumanBytes(bytes_processed),
                HumanBytes(throughput)
            ));

            // Create and save tree
            let mut tree = Tree::new();
            for node in processed_nodes {
                tree.add_node(node);
            }

            let tree_id = repo.save_tree(&tree).await?;

            // Create snapshot with optional hostname override
            let mut snapshot = Snapshot::new(paths.clone(), tree_id);

            if let Some(parent_id) = &self.parent {
                snapshot = snapshot.with_parent(parent_id.clone());
            }

            snapshot = snapshot.with_tags(self.tag.clone());
            snapshot = snapshot.with_excludes(self.exclude.clone());

            // Apply hostname override if specified
            if let Some(hostname) = &self.hostname {
                snapshot.hostname = hostname.clone();
            }

            // Save snapshot
            repo.save_snapshot(&snapshot).await?;

            // Save index to disk
            repo.save_index().await?;

            if failed_files > 0 {
                println!("Backup completed with {} failed files", failed_files);
            } else {
                println!("Backup completed successfully!");
            }
            println!("Snapshot: {}", snapshot.short_id());
            println!(
                "Files: {} | Dirs: {} | Symlinks: {}",
                total_files, total_dirs, total_symlinks
            );
            if total_hardlinks > 0 {
                println!("Hardlinks: {}", total_hardlinks);
            }
            if failed_files > 0 {
                println!("Failed: {}", failed_files);
            }
            if skipped_large > 0 {
                println!("Skipped (large): {}", skipped_large);
            }
            println!(
                "Size: {} | New chunks: {} | Dedup chunks: {}",
                HumanBytes(total_size),
                new_chunks,
                dedup_chunks
            );
            println!(
                "Time: {} @ {}/s",
                HumanDuration(elapsed),
                HumanBytes(throughput)
            );
            println!("Tree: {}", tree_id.short_string());
        } else {
            println!(
                "Dry run completed - would backup {} files, {} dirs, {} symlinks ({})",
                total_files,
                total_dirs,
                total_symlinks,
                HumanBytes(total_size)
            );
        }

        Ok(())
    }

    /// Builds a GlobSet from exclude patterns.
    fn build_exclude_matcher(&self) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();

        for pattern in &self.exclude {
            let glob = Glob::new(pattern)
                .map_err(|e| anyhow!("Invalid exclude pattern '{}': {}", pattern, e))?;
            builder.add(glob);
        }

        builder
            .build()
            .map_err(|e| anyhow!("Failed to build exclude matcher: {}", e))
    }

    /// Checks if a path matches any exclude pattern.
    fn should_exclude(&self, path: &Path, excludes: &GlobSet) -> bool {
        if excludes.is_empty() {
            return false;
        }

        // Check path as-is
        if excludes.is_match(path) {
            return true;
        }

        // Also check just the file/dir name
        if let Some(name) = path.file_name()
            && excludes.is_match(name)
        {
            return true;
        }

        false
    }

    /// Checks if directory contains any exclude-if-present marker files.
    fn check_exclude_if_present(&self, path: &Path) -> bool {
        if self.exclude_if_present.is_empty() {
            return false;
        }

        // Only check for directories
        let dir = if path.is_dir() {
            path
        } else if let Some(parent) = path.parent() {
            parent
        } else {
            return false;
        };

        for marker in &self.exclude_if_present {
            if dir.join(marker).exists() {
                return true;
            }
        }

        false
    }

    /// Process a file and return (chunk_refs, new_chunks_count, dedup_chunks_count)
    async fn process_file_with_stats(
        &self,
        repo: &Repository,
        chunker: &Chunker,
        pack_manager: &mut PackManager,
        file_path: &PathBuf,
    ) -> Result<(Vec<ghostsnap_core::ChunkRef>, u64, u64)> {
        let file_data = fs::read(file_path).await?;
        let chunks = chunker.chunk_data(&file_data);
        let mut chunk_refs = Vec::new();
        let mut new_count = 0u64;
        let mut dedup_count = 0u64;

        for chunk in chunks {
            let chunk_id = chunk.id();

            // Check if chunk already exists (deduplication)
            if !repo.has_chunk(&chunk_id).await? {
                if let Some(finished_pack) = pack_manager.add_chunk(chunk_id, chunk.data())? {
                    self.save_pack_and_index(repo, &finished_pack).await?;
                }
                new_count += 1;
            } else {
                dedup_count += 1;
            }

            chunk_refs.push(ghostsnap_core::ChunkRef {
                id: chunk_id,
                offset: 0,
                length: chunk.data().len() as u32,
            });
        }

        Ok((chunk_refs, new_count, dedup_count))
    }

    async fn save_pack_and_index(&self, repo: &Repository, pack: &PackFile) -> Result<()> {
        repo.save_pack(pack).await?;

        for (chunk_id, chunk_entry) in &pack.chunks {
            repo.save_chunk_location(
                chunk_id,
                &pack.header.pack_id,
                chunk_entry.offset,
                chunk_entry.length,
            )
            .await?;
        }

        info!(
            "Saved pack: {} with {} chunks",
            pack.header.pack_id,
            pack.chunks.len()
        );
        Ok(())
    }
}

/// Read extended attributes from a file (Unix only).
#[cfg(unix)]
fn read_xattrs(path: &Path) -> Option<HashMap<String, Vec<u8>>> {
    let attrs: Vec<_> = match xattr::list(path) {
        Ok(iter) => iter.collect(),
        Err(_) => return None,
    };

    if attrs.is_empty() {
        return None;
    }

    let mut result = HashMap::new();
    for attr_name in attrs {
        if let Ok(Some(value)) = xattr::get(path, &attr_name) {
            // Convert OsString to String, skipping non-UTF8 names
            if let Some(name_str) = attr_name.to_str() {
                result.insert(name_str.to_string(), value);
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(not(unix))]
fn read_xattrs(_path: &Path) -> Option<HashMap<String, Vec<u8>>> {
    None
}

/// Detect sparse file holes using SEEK_HOLE/SEEK_DATA (Unix only).
#[cfg(unix)]
fn detect_sparse_holes(path: &Path, file_size: u64) -> Option<Vec<(u64, u64)>> {
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    if file_size == 0 {
        return None;
    }

    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return None,
    };

    let fd = file.as_raw_fd();
    let mut holes = Vec::new();
    let mut pos: i64 = 0;

    loop {
        // Find next hole
        let hole_start = unsafe { libc::lseek(fd, pos, libc::SEEK_HOLE) };
        if hole_start < 0 || hole_start as u64 >= file_size {
            break;
        }

        // Find end of hole (next data)
        let hole_end = unsafe { libc::lseek(fd, hole_start, libc::SEEK_DATA) };
        let hole_end = if hole_end < 0 {
            file_size as i64
        } else {
            hole_end
        };

        if hole_end > hole_start {
            holes.push((hole_start as u64, (hole_end - hole_start) as u64));
        }

        pos = hole_end;
        if pos as u64 >= file_size {
            break;
        }
    }

    if holes.is_empty() { None } else { Some(holes) }
}

#[cfg(not(unix))]
fn detect_sparse_holes(_path: &Path, _file_size: u64) -> Option<Vec<(u64, u64)>> {
    None
}
