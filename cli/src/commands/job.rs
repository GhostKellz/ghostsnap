//! Job command for config-driven backup operations.
//!
//! The job command provides a declarative way to run backup operations
//! using TOML configuration files.
//!
//! ## Usage
//!
//! ```bash
//! ghostsnap job list                    # List all jobs
//! ghostsnap job show nightly-web        # Show job details
//! ghostsnap job validate nightly-web    # Validate job config
//! ghostsnap job run nightly-web         # Run a backup job
//! ghostsnap job run --all               # Run all jobs
//! ```

use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};
use ghostsnap_core::lock::{LockManager, LockType};
use ghostsnap_core::storage::RepositoryLocation;
use ghostsnap_core::Repository;
use globset::{Glob, GlobSet, GlobSetBuilder};
use indicatif::{HumanBytes, HumanDuration};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::config::{JobConfig, ResolvedJob};
use crate::hooks::{HookConfig, execute_hook_with_output};

/// Job command for running config-driven backups.
#[derive(Args)]
pub struct JobCommand {
    /// Path to the job configuration file.
    #[arg(long, short = 'c', env = "GHOSTSNAP_CONFIG")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    subcommand: JobSubcommand,
}

#[derive(Subcommand)]
enum JobSubcommand {
    /// List all configured jobs.
    List(JobListCommand),

    /// Show details of a specific job.
    Show(JobShowCommand),

    /// Validate a job configuration.
    Validate(JobValidateCommand),

    /// Run a backup job.
    Run(JobRunCommand),
}

impl JobCommand {
    pub async fn run(&self, cli: &crate::Cli) -> Result<()> {
        match &self.subcommand {
            JobSubcommand::List(cmd) => cmd.run(&self.config).await,
            JobSubcommand::Show(cmd) => cmd.run(&self.config).await,
            JobSubcommand::Validate(cmd) => cmd.run(&self.config).await,
            JobSubcommand::Run(cmd) => cmd.run(&self.config, cli).await,
        }
    }
}

/// Load config from specified path or search default locations.
fn load_config(config_path: &Option<PathBuf>) -> Result<(JobConfig, PathBuf)> {
    match config_path {
        Some(path) => {
            let config = JobConfig::load(path)?;
            Ok((config, path.clone()))
        }
        None => JobConfig::find_and_load(),
    }
}

// === List Command ===

#[derive(Args)]
struct JobListCommand {}

impl JobListCommand {
    async fn run(&self, config_path: &Option<PathBuf>) -> Result<()> {
        let (config, path) = load_config(config_path)?;

        println!("Configuration: {}", path.display());
        println!();

        if config.jobs.is_empty() {
            println!("No jobs configured.");
            return Ok(());
        }

        println!("Jobs:");
        for (name, job) in &config.jobs {
            let repo = job
                .repository
                .as_ref()
                .or(config.defaults.repository.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("(no repository)");

            let paths_count = job.paths.len() + job.extra_paths.len();
            let tags_str = if job.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", job.tags.join(", "))
            };

            println!("  {} -> {} ({} paths){}", name, repo, paths_count, tags_str);
        }

        Ok(())
    }
}

// === Show Command ===

#[derive(Args)]
struct JobShowCommand {
    /// Name of the job to show.
    name: String,
}

impl JobShowCommand {
    async fn run(&self, config_path: &Option<PathBuf>) -> Result<()> {
        let (config, _path) = load_config(config_path)?;

        let job = config
            .get_job(&self.name)
            .ok_or_else(|| anyhow!("Job '{}' not found", self.name))?;

        let resolved = ResolvedJob::resolve(&self.name, job, &config.defaults)?;

        println!("Job: {}", self.name);
        println!();

        println!("Repository: {}", resolved.repository);

        if let Some(env) = &resolved.password_env {
            println!("Password: from ${}", env);
        } else if let Some(file) = &resolved.password_file {
            println!("Password: from {}", file.display());
        }

        println!();
        println!("Paths:");
        for path in &resolved.paths {
            println!("  - {}", path.display());
        }

        if !resolved.tags.is_empty() {
            println!();
            println!("Tags: {}", resolved.tags.join(", "));
        }

        if !resolved.exclude.is_empty() {
            println!();
            println!("Excludes:");
            for pattern in &resolved.exclude {
                println!("  - {}", pattern);
            }
        }

        if resolved.pre_hook.is_some() || resolved.post_hook.is_some() {
            println!();
            println!("Hooks:");
            if let Some(ref hook) = resolved.pre_hook {
                println!("  pre_hook: {} (timeout: {:?})", truncate(hook, 50), resolved.pre_hook_timeout);
            }
            if let Some(ref hook) = resolved.post_hook {
                println!("  post_hook: {} (timeout: {:?})", truncate(hook, 50), resolved.post_hook_timeout);
            }
        }

        if resolved.has_retention_policy() {
            println!();
            println!("Retention:");
            if let Some(n) = resolved.keep_last {
                println!("  keep_last: {}", n);
            }
            if let Some(n) = resolved.keep_hourly {
                println!("  keep_hourly: {}", n);
            }
            if let Some(n) = resolved.keep_daily {
                println!("  keep_daily: {}", n);
            }
            if let Some(n) = resolved.keep_weekly {
                println!("  keep_weekly: {}", n);
            }
            if let Some(n) = resolved.keep_monthly {
                println!("  keep_monthly: {}", n);
            }
            if let Some(n) = resolved.keep_yearly {
                println!("  keep_yearly: {}", n);
            }
            if resolved.prune {
                println!("  prune: enabled");
            }
        }

        Ok(())
    }
}

// === Validate Command ===

#[derive(Args)]
struct JobValidateCommand {
    /// Name of the job to validate.
    name: String,
}

impl JobValidateCommand {
    async fn run(&self, config_path: &Option<PathBuf>) -> Result<()> {
        let (config, path) = load_config(config_path)?;

        println!("Validating job '{}' from {}", self.name, path.display());
        println!();

        let job = config
            .get_job(&self.name)
            .ok_or_else(|| anyhow!("Job '{}' not found", self.name))?;

        let resolved = ResolvedJob::resolve(&self.name, job, &config.defaults)?;

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check repository
        print!("Repository: ");
        match RepositoryLocation::parse(&resolved.repository) {
            Ok(_) => println!("OK ({})", resolved.repository),
            Err(e) => {
                println!("ERROR");
                errors.push(format!("Invalid repository: {}", e));
            }
        }

        // Check password source
        print!("Password source: ");
        if resolved.password_env.is_some() || resolved.password_file.is_some() {
            if let Some(ref file) = resolved.password_file {
                if file.exists() {
                    println!("OK (file: {})", file.display());
                } else {
                    println!("WARNING");
                    warnings.push(format!("Password file does not exist: {}", file.display()));
                }
            } else if let Some(ref env) = resolved.password_env {
                if std::env::var(env).is_ok() {
                    println!("OK (env: ${})", env);
                } else {
                    println!("WARNING");
                    warnings.push(format!("Environment variable ${} not set", env));
                }
            }
        } else {
            println!("ERROR");
            errors.push("No password source configured (password_env or password_file)".to_string());
        }

        // Check paths
        print!("Paths: ");
        let mut missing_paths = Vec::new();
        for path in &resolved.paths {
            if !path.exists() {
                missing_paths.push(path.display().to_string());
            }
        }

        if missing_paths.is_empty() {
            println!("OK ({} paths)", resolved.paths.len());
        } else if resolved.require_paths_exist {
            println!("ERROR");
            for p in &missing_paths {
                errors.push(format!("Path does not exist: {}", p));
            }
        } else {
            println!("WARNING");
            for p in &missing_paths {
                warnings.push(format!("Path does not exist: {}", p));
            }
        }

        // Check shell
        print!("Shell: ");
        let shell_path = PathBuf::from(&resolved.shell);
        if shell_path.exists() {
            println!("OK ({})", resolved.shell);
        } else {
            println!("WARNING");
            warnings.push(format!("Shell not found: {}", resolved.shell));
        }

        // Check hooks
        if resolved.pre_hook.is_some() || resolved.post_hook.is_some() {
            println!("Hooks: configured");
        }

        // Check retention
        if resolved.has_retention_policy() {
            println!("Retention: configured");
        } else {
            warnings.push("No retention policy configured".to_string());
        }

        println!();

        // Print summary
        if !warnings.is_empty() {
            println!("Warnings:");
            for w in &warnings {
                println!("  - {}", w);
            }
            println!();
        }

        if !errors.is_empty() {
            println!("Errors:");
            for e in &errors {
                println!("  - {}", e);
            }
            println!();
            return Err(anyhow!("Validation failed with {} error(s)", errors.len()));
        }

        println!("Validation passed!");
        Ok(())
    }
}

// === Run Command ===

#[derive(Args)]
struct JobRunCommand {
    /// Name of the job to run.
    name: Option<String>,

    /// Run all jobs.
    #[arg(long)]
    all: bool,

    /// Dry run - don't actually backup.
    #[arg(long, short = 'n')]
    dry_run: bool,
}

impl JobRunCommand {
    async fn run(&self, config_path: &Option<PathBuf>, cli: &crate::Cli) -> Result<()> {
        let (config, path) = load_config(config_path)?;

        if self.all {
            // Run all jobs
            let job_names: Vec<String> = config.jobs.keys().cloned().collect();

            if job_names.is_empty() {
                return Err(anyhow!("No jobs configured in {}", path.display()));
            }

            println!("Running {} jobs from {}", job_names.len(), path.display());
            println!();

            let mut success_count = 0;
            let mut failure_count = 0;

            for name in &job_names {
                match self.run_single_job(&config, name, cli).await {
                    Ok(_) => success_count += 1,
                    Err(e) => {
                        println!("Job '{}' failed: {}", name, e);
                        failure_count += 1;
                    }
                }
                println!();
            }

            println!("Completed: {} succeeded, {} failed", success_count, failure_count);

            if failure_count > 0 {
                return Err(anyhow!("{} job(s) failed", failure_count));
            }
        } else {
            // Run single job
            let name = self
                .name
                .as_ref()
                .ok_or_else(|| anyhow!("Job name required. Use --all to run all jobs."))?;

            self.run_single_job(&config, name, cli).await?;
        }

        Ok(())
    }

    async fn run_single_job(
        &self,
        config: &JobConfig,
        name: &str,
        cli: &crate::Cli,
    ) -> Result<()> {
        let job = config
            .get_job(name)
            .ok_or_else(|| anyhow!("Job '{}' not found", name))?;

        let mut resolved = ResolvedJob::resolve(name, job, &config.defaults)?;

        // Override dry_run from CLI
        if self.dry_run {
            resolved.dry_run = true;
        }

        let total_start = Instant::now();

        println!("Job: {}", resolved.name);
        println!("Repository: {}", resolved.repository);
        println!("{}", "─".repeat(50));

        // Resolve password
        let password = resolved.resolve_password()?;

        // Validate paths exist
        if resolved.require_paths_exist {
            for path in &resolved.paths {
                if !path.exists() {
                    return Err(anyhow!("Required path does not exist: {}", path.display()));
                }
            }
        }

        // Parse repository location
        let repo_location = RepositoryLocation::parse(&resolved.repository)
            .map_err(|e| anyhow!("Invalid repository: {}", e))?;

        // Execute pre-hook
        if let Some(ref hook_cmd) = resolved.pre_hook {
            let hook_config = HookConfig {
                command: hook_cmd.clone(),
                timeout: resolved.pre_hook_timeout,
                shell: resolved.shell.clone(),
                working_dir: resolved.working_directory.clone(),
            };

            let result = execute_hook_with_output("Pre-hook", &hook_config, cli.verbose).await?;

            if !result.success && resolved.stop_on_pre_hook_failure {
                return Err(anyhow!("Pre-hook failed, aborting job"));
            }
        }

        // Open repository
        info!("Opening repository: {}", resolved.repository);
        let repo = Repository::open_at_location(repo_location.clone(), &password).await?;

        // Acquire lock (for local repos)
        let _lock = if let Some(repo_path) = repo.local_path() {
            let lock_manager = LockManager::new(repo_path);
            Some(lock_manager.acquire(LockType::Exclusive, "job").await?)
        } else {
            warn!("Repository locking not supported for remote repositories");
            None
        };

        // Execute backup
        let backup_result = self.run_backup(&repo, &resolved, cli).await;

        let snapshot_id = match backup_result {
            Ok(id) => {
                println!("Backup: OK");
                println!("  Snapshot: {}", &id[..8]);
                Some(id)
            }
            Err(e) => {
                println!("Backup: FAILED");
                println!("  Error: {}", e);
                None
            }
        };

        // Execute forget if retention configured
        if snapshot_id.is_some() && resolved.has_retention_policy() {
            match self.run_forget(&repo, &resolved).await {
                Ok((kept, removed)) => {
                    println!("Forget: OK");
                    println!("  Kept: {}, Removed: {}", kept, removed);
                }
                Err(e) => {
                    println!("Forget: FAILED");
                    println!("  Error: {}", e);
                }
            }
        }

        // Execute prune if enabled
        if snapshot_id.is_some() && resolved.prune {
            match self.run_prune(&repo).await {
                Ok((packs_removed, bytes_freed)) => {
                    println!("Prune: OK");
                    if packs_removed > 0 {
                        println!("  Removed: {} packs ({})", packs_removed, HumanBytes(bytes_freed));
                    } else {
                        println!("  Nothing to prune");
                    }
                }
                Err(e) => {
                    println!("Prune: FAILED");
                    println!("  Error: {}", e);
                }
            }
        }

        // Execute post-hook (always runs)
        if let Some(ref hook_cmd) = resolved.post_hook {
            let hook_config = HookConfig {
                command: hook_cmd.clone(),
                timeout: resolved.post_hook_timeout,
                shell: resolved.shell.clone(),
                working_dir: resolved.working_directory.clone(),
            };

            let _ = execute_hook_with_output("Post-hook", &hook_config, cli.verbose).await;
        }

        let total_duration = total_start.elapsed();
        println!("{}", "─".repeat(50));
        println!("Total duration: {}", HumanDuration(total_duration));

        if snapshot_id.is_none() {
            return Err(anyhow!("Backup failed"));
        }

        Ok(())
    }

    async fn run_backup(
        &self,
        repo: &Repository,
        job: &ResolvedJob,
        _cli: &crate::Cli,
    ) -> Result<String> {
        use ghostsnap_core::chunker::Chunker;
        use ghostsnap_core::pack::PackManager;
        use ghostsnap_core::snapshot::{Snapshot, Tree};
        use ghostsnap_core::{ChunkRef, NodeType, TreeNode};
        use walkdir::WalkDir;

        if job.dry_run {
            println!("  (dry run - skipping actual backup)");
            return Ok("00000000-0000-0000-0000-000000000000".to_string());
        }

        let chunker = Chunker::new_default();
        let mut pack_manager = PackManager::new(64 * 1024 * 1024);
        let mut tree = Tree::new();

        let mut files_new = 0u64;
        let mut files_unchanged = 0u64;
        let mut bytes_processed = 0u64;
        let mut bytes_added = 0u64;

        // Build glob-based exclude matcher (same as backup command)
        let excludes = self.build_exclude_matcher(&job.exclude)?;

        for source_path in &job.paths {
            if !source_path.exists() {
                if job.require_paths_exist {
                    return Err(anyhow!("Path does not exist: {}", source_path.display()));
                }
                warn!("Skipping non-existent path: {}", source_path.display());
                continue;
            }

            let mut walker = WalkDir::new(source_path).follow_links(false);

            // Honor one_file_system option
            if job.one_file_system {
                walker = walker.same_file_system(true);
            }

            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                let relative = path.strip_prefix(source_path).unwrap_or(path);

                // Check glob-based excludes
                if self.should_exclude(path, &excludes) {
                    debug!("Excluding (glob): {}", path.display());
                    continue;
                }

                // Check exclude_if_present markers
                if self.check_exclude_if_present(path, &job.exclude_if_present) {
                    debug!("Excluding (marker file): {}", path.display());
                    continue;
                }

                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                #[cfg(unix)]
                let (mode, uid, gid) = {
                    use std::os::unix::fs::MetadataExt;
                    (metadata.mode(), metadata.uid(), metadata.gid())
                };
                #[cfg(not(unix))]
                let (mode, uid, gid) = (0o644, 0, 0);

                let mtime = metadata
                    .modified()
                    .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
                    .unwrap_or(0);

                let node_type = if metadata.is_file() {
                    NodeType::File
                } else if metadata.is_dir() {
                    NodeType::Directory
                } else if metadata.is_symlink() {
                    NodeType::Symlink
                } else {
                    continue;
                };

                let mut chunks = Vec::new();

                if metadata.is_file() {
                    let data = std::fs::read(path)?;
                    bytes_processed += data.len() as u64;

                    let mut is_new = false;
                    for chunk in chunker.chunk_data(&data) {
                        let chunk_id = chunk.id();
                        if !repo.has_chunk(&chunk_id).await? {
                            is_new = true;
                            bytes_added += chunk.data().len() as u64;
                            if let Some(pack) = pack_manager.add_chunk(chunk_id, chunk.data())? {
                                repo.save_pack(&pack).await?;
                                for (cid, ce) in &pack.chunks {
                                    repo.save_chunk_location(cid, &pack.header.pack_id, ce.offset, ce.length)
                                        .await?;
                                }
                            }
                        }
                        chunks.push(ChunkRef {
                            id: chunk_id,
                            offset: 0,
                            length: chunk.data().len() as u32,
                        });
                    }

                    if is_new {
                        files_new += 1;
                    } else {
                        files_unchanged += 1;
                    }
                }

                tree.add_node(TreeNode {
                    name: relative.to_string_lossy().to_string(),
                    node_type,
                    mode,
                    uid,
                    gid,
                    size: metadata.len(),
                    mtime,
                    link_target: None,
                    subtree_id: None,
                    chunks,
                    xattr: None,
                    sparse_holes: None,
                    inode: None,
                    nlink: None,
                    hardlink_target: None,
                });
            }
        }

        // Finish remaining pack
        if let Some(pack) = pack_manager.finish_current_pack() {
            repo.save_pack(&pack).await?;
            for (cid, ce) in &pack.chunks {
                repo.save_chunk_location(cid, &pack.header.pack_id, ce.offset, ce.length)
                    .await?;
            }
        }

        // Save tree and snapshot
        let tree_id = repo.save_tree(&tree).await?;
        let mut snapshot = Snapshot::new(job.paths.clone(), tree_id);

        // Apply tags
        if !job.tags.is_empty() {
            snapshot = snapshot.with_tags(job.tags.clone());
        }

        // Apply hostname
        if let Some(ref hostname) = job.hostname {
            snapshot.hostname = hostname.clone();
        }

        repo.save_snapshot(&snapshot).await?;
        repo.save_index().await?;

        println!("  Files: {} new, {} unchanged", files_new, files_unchanged);
        println!(
            "  Size: {} processed, {} added",
            HumanBytes(bytes_processed),
            HumanBytes(bytes_added)
        );

        Ok(snapshot.id)
    }

    async fn run_forget(&self, repo: &Repository, job: &ResolvedJob) -> Result<(usize, usize)> {
        use chrono::Datelike;
        use std::collections::HashSet;

        let snapshot_ids = repo.list_snapshots().await?;
        let mut snapshots = Vec::new();

        for id in snapshot_ids {
            let snapshot = repo.load_snapshot(&id).await?;
            snapshots.push(snapshot);
        }

        // Sort by time, newest first
        snapshots.sort_by_key(|s| std::cmp::Reverse(s.time));

        let mut keep_ids: HashSet<String> = HashSet::new();

        // Keep last N
        if let Some(n) = job.keep_last {
            for snapshot in snapshots.iter().take(n as usize) {
                keep_ids.insert(snapshot.id.clone());
            }
        }

        // Keep daily
        if let Some(n) = job.keep_daily {
            let mut days_seen = HashSet::new();
            for snapshot in &snapshots {
                let day = snapshot.time.date_naive();
                if days_seen.len() < n as usize && !days_seen.contains(&day) {
                    days_seen.insert(day);
                    keep_ids.insert(snapshot.id.clone());
                }
            }
        }

        // Keep weekly
        if let Some(n) = job.keep_weekly {
            let mut weeks_seen = HashSet::new();
            for snapshot in &snapshots {
                let week = snapshot.time.iso_week();
                let week_key = (week.year(), week.week());
                if weeks_seen.len() < n as usize && !weeks_seen.contains(&week_key) {
                    weeks_seen.insert(week_key);
                    keep_ids.insert(snapshot.id.clone());
                }
            }
        }

        // Keep monthly
        if let Some(n) = job.keep_monthly {
            let mut months_seen = HashSet::new();
            for snapshot in &snapshots {
                let month_key = (snapshot.time.year(), snapshot.time.month());
                if months_seen.len() < n as usize && !months_seen.contains(&month_key) {
                    months_seen.insert(month_key);
                    keep_ids.insert(snapshot.id.clone());
                }
            }
        }

        // Keep yearly
        if let Some(n) = job.keep_yearly {
            let mut years_seen = HashSet::new();
            for snapshot in &snapshots {
                let year = snapshot.time.year();
                if years_seen.len() < n as usize && !years_seen.contains(&year) {
                    years_seen.insert(year);
                    keep_ids.insert(snapshot.id.clone());
                }
            }
        }

        // If no policy, keep all
        if !job.has_retention_policy() {
            for snapshot in &snapshots {
                keep_ids.insert(snapshot.id.clone());
            }
        }

        // Delete snapshots not in keep set
        let mut removed = 0;
        for snapshot in &snapshots {
            if !keep_ids.contains(&snapshot.id) {
                repo.delete_snapshot(&snapshot.id).await?;
                removed += 1;
            }
        }

        Ok((keep_ids.len(), removed))
    }

    async fn run_prune(&self, repo: &Repository) -> Result<(usize, u64)> {
        use std::collections::HashSet;

        // Collect all referenced chunks
        let mut referenced_chunks: HashSet<ghostsnap_core::ChunkID> = HashSet::new();

        let snapshot_ids = repo.list_snapshots().await?;
        for snapshot_id in &snapshot_ids {
            let snapshot = repo.load_snapshot(snapshot_id).await?;
            let tree = repo.load_tree(&snapshot.tree).await?;

            for node in &tree.nodes {
                for chunk_ref in &node.chunks {
                    referenced_chunks.insert(chunk_ref.id);
                }
            }
        }

        // Find packs with no referenced chunks
        let all_packs = repo.list_packs().await?;
        let index = repo.index();
        let index_guard = index.read().await;

        let mut packs_to_delete = Vec::new();
        let mut bytes_freed = 0u64;

        for pack_id in &all_packs {
            // Check if any chunk in this pack is referenced
            let mut has_referenced = false;
            for (chunk_id, location) in index_guard.iter_chunks() {
                if &location.pack_id == pack_id && referenced_chunks.contains(chunk_id) {
                    has_referenced = true;
                    break;
                }
            }

            if !has_referenced {
                if let Ok(size) = repo.pack_size(pack_id).await {
                    bytes_freed += size;
                }
                packs_to_delete.push(pack_id.clone());
            }
        }
        drop(index_guard);

        // Delete orphaned packs
        for pack_id in &packs_to_delete {
            repo.delete_pack(pack_id).await?;
        }

        // Save index
        repo.save_index().await?;

        Ok((packs_to_delete.len(), bytes_freed))
    }

    /// Builds a GlobSet from exclude patterns.
    fn build_exclude_matcher(&self, patterns: &[String]) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();

        for pattern in patterns {
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
    fn check_exclude_if_present(&self, path: &Path, markers: &[String]) -> bool {
        if markers.is_empty() {
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

        for marker in markers {
            if dir.join(marker).exists() {
                return true;
            }
        }

        false
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s).trim();
    if first_line.len() <= max_len {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max_len - 3])
    }
}
