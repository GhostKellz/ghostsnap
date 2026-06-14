use anyhow::{Result, anyhow};
use clap::Args;
use ghostsnap_core::{NodeType, Repository, TreeNode};
use indicatif::{HumanBytes, HumanDuration, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs;
use tracing::{debug, info, warn};

#[derive(Args)]
pub struct RestoreCommand {
    #[arg(help = "Snapshot ID (full or short prefix)")]
    snapshot_id: String,

    #[arg(short = 't', long, help = "Target directory for restore")]
    target: String,

    #[arg(help = "Specific paths to restore (optional)")]
    paths: Vec<String>,

    #[arg(long, help = "Don't restore file permissions")]
    no_permissions: bool,

    #[arg(long, help = "Don't restore ownership (uid/gid)")]
    no_ownership: bool,

    #[arg(long, help = "Overwrite existing files")]
    overwrite: bool,

    #[arg(long, short = 'n', help = "Dry run - don't write any files")]
    dry_run: bool,

    #[arg(long, help = "Don't restore extended attributes")]
    no_xattr: bool,

    #[arg(long, help = "Don't restore file timestamps (mtime)")]
    no_timestamps: bool,

    #[arg(long, help = "Restore sparse files with holes")]
    sparse: bool,

    #[arg(long, help = "Verify restored files by recomputing their hash")]
    verify: bool,

    #[arg(
        long,
        help = "Don't restore hardlinks as hardlinks (create copies instead)"
    )]
    no_hardlinks: bool,
}

impl RestoreCommand {
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

        // Support short snapshot IDs
        let full_snapshot_id = self.resolve_snapshot_id(&repo, &self.snapshot_id).await?;

        info!("Loading snapshot: {}", full_snapshot_id);
        let snapshot = repo.load_snapshot(&full_snapshot_id).await?;

        let target_path = PathBuf::from(&self.target);
        if !target_path.exists() {
            if self.dry_run {
                println!("Would create target directory: {}", target_path.display());
            } else {
                fs::create_dir_all(&target_path).await?;
            }
        }

        println!("Restoring snapshot: {}", snapshot.short_id());
        println!("Created: {}", snapshot.time.format("%Y-%m-%d %H:%M:%S UTC"));
        println!("Host: {}", snapshot.hostname);
        println!("User: {}", snapshot.username);
        println!("Target: {}", target_path.display());

        if self.dry_run {
            println!("DRY RUN - no files will be written");
        }

        // Load the tree
        let tree = repo.load_tree(&snapshot.tree).await?;

        // Build a lookup map for finding original files (needed for hardlink restoration)
        let node_by_name: HashMap<String, &TreeNode> = tree
            .nodes
            .iter()
            .map(|node| (node.name.clone(), node))
            .collect();

        // Filter nodes to restore
        let mut nodes_to_restore: Vec<_> = if self.paths.is_empty() {
            tree.nodes.iter().collect()
        } else {
            tree.nodes
                .iter()
                .filter(|node| {
                    self.paths.iter().any(|p| {
                        let p = p.trim_end_matches('/');
                        // Exact match or proper directory prefix (with path separator)
                        node.name == p || node.name.starts_with(&format!("{}/", p))
                    })
                })
                .collect()
        };

        if nodes_to_restore.is_empty() {
            println!("No files to restore");
            return Ok(());
        }

        // Sort nodes: directories first (by depth), then files and symlinks
        // This ensures parent directories are created before their contents
        nodes_to_restore.sort_by(|a, b| {
            let a_is_dir = matches!(a.node_type, NodeType::Directory);
            let b_is_dir = matches!(b.node_type, NodeType::Directory);

            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        // Count by type
        let dir_count = nodes_to_restore
            .iter()
            .filter(|n| n.node_type == NodeType::Directory)
            .count();
        let file_count = nodes_to_restore
            .iter()
            .filter(|n| n.node_type == NodeType::File)
            .count();
        let symlink_count = nodes_to_restore
            .iter()
            .filter(|n| n.node_type == NodeType::Symlink)
            .count();

        // Count hardlinks
        let hardlink_count = nodes_to_restore
            .iter()
            .filter(|n| n.hardlink_target.is_some())
            .count();

        println!(
            "Restoring {} dirs, {} files, {} symlinks...",
            dir_count, file_count, symlink_count
        );
        if hardlink_count > 0 {
            println!("  ({} hardlinks)", hardlink_count);
        }

        // Calculate total bytes to restore
        let total_bytes: u64 = nodes_to_restore
            .iter()
            .filter(|n| n.node_type == NodeType::File)
            .map(|n| n.size)
            .sum();

        let pb = ProgressBar::new(total_bytes);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta}) {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );

        let start_time = Instant::now();
        let mut restored_count = 0;
        let mut skipped_count = 0;
        let mut failed_count = 0;
        let mut verified_count = 0;
        let mut verify_failed_count = 0;
        let mut bytes_restored = 0u64;
        let mut hardlinks_restored = 0;

        // Track directories for later timestamp restoration
        let mut directories: Vec<(PathBuf, &TreeNode)> = Vec::new();

        // Track restored files for hardlink creation (path -> dest_path)
        let mut restored_files: HashMap<String, PathBuf> = HashMap::new();

        for node in &nodes_to_restore {
            pb.set_message(node.name.clone());

            let dest_path = target_path.join(&node.name);

            // Check if file exists
            if dest_path.exists() && !self.overwrite && !self.dry_run {
                skipped_count += 1;
                debug!("Skipping existing: {}", node.name);
                if node.node_type == NodeType::File {
                    bytes_restored += node.size;
                    pb.set_position(bytes_restored);
                }
                continue;
            }

            let result = match node.node_type {
                NodeType::Directory => {
                    if self.dry_run {
                        println!("Would create directory: {}", dest_path.display());
                        Ok(())
                    } else {
                        directories.push((dest_path.clone(), node));
                        self.restore_directory(node, &dest_path).await
                    }
                }
                NodeType::File => {
                    if self.dry_run {
                        if let Some(ref target) = node.hardlink_target {
                            println!(
                                "Would create hardlink: {} -> {}",
                                dest_path.display(),
                                target
                            );
                        } else {
                            println!(
                                "Would restore file: {} ({})",
                                dest_path.display(),
                                HumanBytes(node.size)
                            );
                        }
                        bytes_restored += node.size;
                        pb.set_position(bytes_restored);
                        Ok(())
                    } else if let Some(ref target) = node.hardlink_target {
                        // This is a hardlink - create it as a link to the original
                        if !self.no_hardlinks {
                            if let Some(original_path) = restored_files.get(target) {
                                self.restore_hardlink(original_path, &dest_path).await
                            } else {
                                // Original file not found - restore as normal file
                                warn!("Hardlink target {} not found, restoring as copy", target);
                                self.restore_file(&repo, node, &dest_path).await
                            }
                        } else {
                            // --no-hardlinks flag: restore as copy of original file
                            if let Some(original_node) = node_by_name.get(target) {
                                self.restore_file(&repo, original_node, &dest_path).await
                            } else {
                                Err(anyhow!(
                                    "Hardlink target '{}' not found in snapshot tree",
                                    target
                                ))
                            }
                        }
                    } else {
                        // Normal file
                        let result = self.restore_file(&repo, node, &dest_path).await;
                        if result.is_ok() {
                            // Track for potential hardlinks
                            restored_files.insert(node.name.clone(), dest_path.clone());
                        }
                        result
                    }
                }
                NodeType::Symlink => {
                    if self.dry_run {
                        let target = node.link_target.as_deref().unwrap_or("(unknown)");
                        println!(
                            "Would create symlink: {} -> {}",
                            dest_path.display(),
                            target
                        );
                        Ok(())
                    } else {
                        self.restore_symlink(node, &dest_path).await
                    }
                }
            };

            match result {
                Ok(_) => {
                    restored_count += 1;
                    if node.hardlink_target.is_some() && !self.no_hardlinks {
                        hardlinks_restored += 1;
                    }

                    // Verify if requested
                    if self.verify && node.node_type == NodeType::File && !self.dry_run {
                        if let Err(e) = self.verify_file(&repo, node, &dest_path).await {
                            warn!("Verification failed for {}: {}", node.name, e);
                            verify_failed_count += 1;
                        } else {
                            verified_count += 1;
                        }
                    }

                    debug!("Successfully restored: {}", node.name);
                }
                Err(e) => {
                    failed_count += 1;
                    warn!("Failed to restore {}: {}", node.name, e);
                }
            }

            if node.node_type == NodeType::File {
                bytes_restored += node.size;
                pb.set_position(bytes_restored);
            }
        }

        // Restore directory timestamps after all contents are written
        // (writing files inside would update the directory mtime)
        if !self.dry_run && !self.no_timestamps {
            for (dir_path, node) in directories.iter().rev() {
                if let Err(e) = self.set_timestamps(dir_path, node.mtime).await {
                    debug!(
                        "Failed to set directory timestamp for {}: {}",
                        dir_path.display(),
                        e
                    );
                }
            }
        }

        let elapsed = start_time.elapsed();
        let throughput = if elapsed.as_secs() > 0 {
            bytes_restored / elapsed.as_secs()
        } else {
            bytes_restored
        };

        pb.finish_with_message(format!(
            "Done ({} @ {}/s)",
            HumanBytes(bytes_restored),
            HumanBytes(throughput)
        ));

        println!("Restore completed!");
        println!(
            "Restored: {} ({} in {})",
            restored_count,
            HumanBytes(bytes_restored),
            HumanDuration(elapsed)
        );
        if hardlinks_restored > 0 {
            println!("Hardlinks: {}", hardlinks_restored);
        }
        if skipped_count > 0 {
            println!("Skipped (existing): {}", skipped_count);
        }
        if failed_count > 0 {
            println!("Failed: {}", failed_count);
        }
        if self.verify {
            println!(
                "Verified: {} | Failed: {}",
                verified_count, verify_failed_count
            );
        }
        println!("Location: {}", target_path.display());

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

    async fn restore_directory(&self, node: &TreeNode, dest_path: &Path) -> Result<()> {
        // Create directory
        fs::create_dir_all(dest_path).await?;

        // Set permissions
        if !self.no_permissions {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let permissions = std::fs::Permissions::from_mode(node.mode);
                fs::set_permissions(dest_path, permissions).await?;
            }
        }

        // Set ownership (requires root)
        if !self.no_ownership {
            self.set_ownership(dest_path, node.uid, node.gid).await?;
        }

        // Restore extended attributes
        if !self.no_xattr
            && let Some(ref xattrs) = node.xattr
        {
            self.restore_xattrs(dest_path, xattrs).await?;
        }

        debug!("Created directory: {}", dest_path.display());
        Ok(())
    }

    async fn restore_file(
        &self,
        repo: &Repository,
        node: &TreeNode,
        dest_path: &Path,
    ) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Reconstruct file from chunks
        let mut file_data = Vec::with_capacity(node.size as usize);

        for chunk_ref in &node.chunks {
            let chunk_data = repo.load_chunk(&chunk_ref.id).await?;
            file_data.extend_from_slice(&chunk_data);
        }

        // Write file
        fs::write(dest_path, &file_data).await?;

        // Punch holes for sparse files if requested
        if self.sparse
            && let Some(ref holes) = node.sparse_holes
            && !holes.is_empty()
        {
            self.punch_holes(dest_path, holes)?;
            debug!(
                "Restored sparse file with {} holes: {}",
                holes.len(),
                dest_path.display()
            );
        }

        // Set permissions
        if !self.no_permissions {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let permissions = std::fs::Permissions::from_mode(node.mode);
                fs::set_permissions(dest_path, permissions).await?;
            }
        }

        // Set ownership
        if !self.no_ownership {
            self.set_ownership(dest_path, node.uid, node.gid).await?;
        }

        // Set timestamps
        if !self.no_timestamps {
            self.set_timestamps(dest_path, node.mtime).await?;
        }

        // Restore extended attributes
        if !self.no_xattr
            && let Some(ref xattrs) = node.xattr
        {
            self.restore_xattrs(dest_path, xattrs).await?;
        }

        debug!(
            "Restored file: {} ({} bytes)",
            dest_path.display(),
            file_data.len()
        );
        Ok(())
    }

    async fn restore_symlink(&self, node: &TreeNode, dest_path: &Path) -> Result<()> {
        let link_target = node
            .link_target
            .as_ref()
            .ok_or_else(|| anyhow!("Symlink {} has no target", node.name))?;

        // Create parent directories if needed
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Remove existing if overwrite is set
        if dest_path.exists() || dest_path.symlink_metadata().is_ok() {
            fs::remove_file(dest_path).await.ok();
        }

        // Create symlink
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(link_target, dest_path)?;
        }

        #[cfg(not(unix))]
        {
            // On Windows, try to create a symlink (may require privileges)
            // For directories, use symlink_dir; for files, use symlink_file
            // Since we don't know the target type, try file first
            if let Err(_) = std::os::windows::fs::symlink_file(link_target, dest_path) {
                std::os::windows::fs::symlink_dir(link_target, dest_path)?;
            }
        }

        // Set ownership on symlink (lchown)
        if !self.no_ownership {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                let path_cstr = std::ffi::CString::new(dest_path.as_os_str().as_bytes())?;
                unsafe {
                    libc::lchown(path_cstr.as_ptr(), node.uid, node.gid);
                }
            }
        }

        debug!(
            "Created symlink: {} -> {}",
            dest_path.display(),
            link_target
        );
        Ok(())
    }

    async fn set_ownership(&self, path: &Path, uid: u32, gid: u32) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;

            // Only attempt if we're running as root
            if unsafe { libc::geteuid() } != 0 {
                return Ok(());
            }

            let path_cstr = std::ffi::CString::new(path.as_os_str().as_bytes())?;
            let result = unsafe { libc::chown(path_cstr.as_ptr(), uid, gid) };

            if result != 0 {
                debug!(
                    "Failed to set ownership on {}: {}",
                    path.display(),
                    std::io::Error::last_os_error()
                );
            }
        }

        #[cfg(not(unix))]
        {
            let _ = (path, uid, gid);
        }

        Ok(())
    }

    async fn set_timestamps(&self, path: &Path, mtime: i64) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;

            let path_cstr = std::ffi::CString::new(path.as_os_str().as_bytes())?;
            let times = [
                libc::timespec {
                    tv_sec: mtime,
                    tv_nsec: 0,
                }, // atime
                libc::timespec {
                    tv_sec: mtime,
                    tv_nsec: 0,
                }, // mtime
            ];

            unsafe {
                libc::utimensat(libc::AT_FDCWD, path_cstr.as_ptr(), times.as_ptr(), 0);
            }
        }

        #[cfg(not(unix))]
        {
            let _ = (path, mtime);
        }

        Ok(())
    }

    async fn restore_xattrs(&self, path: &Path, xattrs: &HashMap<String, Vec<u8>>) -> Result<()> {
        #[cfg(unix)]
        {
            for (name, value) in xattrs {
                if let Err(e) = xattr::set(path, name, value) {
                    debug!("Failed to set xattr {} on {}: {}", name, path.display(), e);
                }
            }
        }

        #[cfg(not(unix))]
        {
            let _ = (path, xattrs);
        }

        Ok(())
    }

    fn punch_holes(&self, path: &Path, holes: &[(u64, u64)]) -> Result<()> {
        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            use std::os::unix::io::AsRawFd;

            let file = OpenOptions::new().write(true).open(path)?;

            let fd = file.as_raw_fd();

            for (offset, length) in holes {
                // FALLOC_FL_PUNCH_HOLE | FALLOC_FL_KEEP_SIZE
                let flags = libc::FALLOC_FL_PUNCH_HOLE | libc::FALLOC_FL_KEEP_SIZE;
                let result = unsafe { libc::fallocate(fd, flags, *offset as i64, *length as i64) };

                if result != 0 {
                    let err = std::io::Error::last_os_error();
                    // EOPNOTSUPP is okay - filesystem doesn't support it
                    if err.raw_os_error() != Some(libc::EOPNOTSUPP) {
                        debug!(
                            "Failed to punch hole at offset {} in {}: {}",
                            offset,
                            path.display(),
                            err
                        );
                    }
                }
            }
        }

        #[cfg(not(unix))]
        {
            let _ = (path, holes);
        }

        Ok(())
    }

    /// Creates a hardlink from dest_path pointing to original_path.
    async fn restore_hardlink(&self, original_path: &Path, dest_path: &Path) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Remove existing if overwrite is set
        if dest_path.exists() {
            fs::remove_file(dest_path).await?;
        }

        // Create hardlink
        #[cfg(unix)]
        {
            std::fs::hard_link(original_path, dest_path)?;
        }

        #[cfg(not(unix))]
        {
            // On non-Unix, fall back to copy
            std::fs::copy(original_path, dest_path)?;
        }

        debug!(
            "Created hardlink: {} -> {}",
            dest_path.display(),
            original_path.display()
        );
        Ok(())
    }

    /// Verifies a restored file by recomputing its content hash.
    async fn verify_file(
        &self,
        repo: &Repository,
        node: &TreeNode,
        dest_path: &Path,
    ) -> Result<()> {
        // Skip verification for hardlinks - they point to already verified files
        if node.hardlink_target.is_some() && !self.no_hardlinks {
            return Ok(());
        }

        let restored_data = fs::read(dest_path).await?;

        // Verify each chunk hash matches
        let mut expected_data = Vec::with_capacity(node.size as usize);
        for chunk_ref in &node.chunks {
            let chunk_data = repo.load_chunk(&chunk_ref.id).await?;
            expected_data.extend_from_slice(&chunk_data);
        }

        if restored_data.len() != expected_data.len() {
            return Err(anyhow!(
                "Size mismatch: expected {} bytes, got {} bytes",
                expected_data.len(),
                restored_data.len()
            ));
        }

        // Compute and compare BLAKE3 hash
        let restored_hash = blake3::hash(&restored_data);
        let expected_hash = blake3::hash(&expected_data);

        if restored_hash != expected_hash {
            return Err(anyhow!(
                "Hash mismatch: expected {}, got {}",
                expected_hash.to_hex(),
                restored_hash.to_hex()
            ));
        }

        debug!(
            "Verified: {} (hash: {})",
            dest_path.display(),
            restored_hash.to_hex().chars().take(8).collect::<String>()
        );
        Ok(())
    }
}
