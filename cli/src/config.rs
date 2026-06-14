//! Job configuration file parsing.
//!
//! Ghostsnap supports TOML-based job configuration files for repeatable backup operations.
//!
//! ## Example Configuration
//!
//! ```toml
//! version = 1
//!
//! [defaults]
//! repository = "s3:my-bucket/backups"
//! password_env = "GHOSTSNAP_PASSWORD"
//!
//! [jobs.nightly-web]
//! paths = ["/etc/nginx", "/var/www"]
//! tags = ["host:web-01", "service:nginx"]
//! keep_daily = 7
//! keep_weekly = 4
//! prune = true
//! ```

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Top-level job configuration file.
#[derive(Debug, Deserialize)]
pub struct JobConfig {
    /// Config file format version (currently 1).
    pub version: u32,

    /// Default settings applied to all jobs.
    #[serde(default)]
    pub defaults: JobDefaults,

    /// Named backup jobs.
    #[serde(default)]
    pub jobs: HashMap<String, Job>,
}

/// Default settings that apply to all jobs unless overridden.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct JobDefaults {
    /// Default repository path.
    pub repository: Option<String>,

    /// Environment variable containing the password.
    pub password_env: Option<String>,

    /// Path to file containing the password.
    pub password_file: Option<PathBuf>,

    /// Default shell for hooks.
    pub shell: Option<String>,
}

/// A single backup job definition.
#[derive(Debug, Deserialize, Clone)]
pub struct Job {
    /// Repository path (overrides defaults).
    pub repository: Option<String>,

    /// Environment variable containing the password.
    pub password_env: Option<String>,

    /// Path to file containing the password.
    pub password_file: Option<PathBuf>,

    /// Paths to back up.
    pub paths: Vec<String>,

    /// Additional paths to back up (e.g., staging directories for dumps).
    #[serde(default)]
    pub extra_paths: Vec<String>,

    /// Tags to apply to the snapshot.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Patterns to exclude from backup.
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Patterns that cause directories to be excluded if present.
    #[serde(default)]
    pub exclude_if_present: Vec<String>,

    /// Override the hostname in snapshot metadata.
    pub hostname: Option<String>,

    /// Stay on a single filesystem (don't cross mount points).
    #[serde(default)]
    pub one_file_system: bool,

    // --- Hooks ---
    /// Command to run before backup.
    pub pre_hook: Option<String>,

    /// Command to run after backup (runs even on failure).
    pub post_hook: Option<String>,

    /// Timeout for pre-hook (e.g., "5m", "30s").
    pub pre_hook_timeout: Option<String>,

    /// Timeout for post-hook.
    pub post_hook_timeout: Option<String>,

    /// Shell to use for hooks (default: /bin/sh).
    pub shell: Option<String>,

    /// Working directory for hooks.
    pub working_directory: Option<PathBuf>,

    // --- Retention ---
    /// Keep the last N snapshots.
    pub keep_last: Option<u32>,

    /// Keep N hourly snapshots.
    pub keep_hourly: Option<u32>,

    /// Keep N daily snapshots.
    pub keep_daily: Option<u32>,

    /// Keep N weekly snapshots.
    pub keep_weekly: Option<u32>,

    /// Keep N monthly snapshots.
    pub keep_monthly: Option<u32>,

    /// Keep N yearly snapshots.
    pub keep_yearly: Option<u32>,

    /// Run prune after forget.
    #[serde(default)]
    pub prune: bool,

    // --- Safety ---
    /// Require all paths to exist before backup (default: true).
    #[serde(default = "default_true")]
    pub require_paths_exist: bool,

    /// Stop the job if pre-hook fails (default: true).
    #[serde(default = "default_true")]
    pub stop_on_pre_hook_failure: bool,

    /// Dry run mode - don't actually backup.
    #[serde(default)]
    pub dry_run: bool,
}

fn default_true() -> bool {
    true
}

impl JobConfig {
    /// Load configuration from a file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: JobConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        if config.version != 1 {
            return Err(anyhow!(
                "Unsupported config version: {}. Expected version 1.",
                config.version
            ));
        }

        Ok(config)
    }

    /// Find and load configuration from default locations.
    ///
    /// Search order:
    /// 1. GHOSTSNAP_CONFIG environment variable
    /// 2. /etc/ghostsnap/jobs.toml
    /// 3. ./ghostsnap.toml
    pub fn find_and_load() -> Result<(Self, PathBuf)> {
        let candidates = Self::config_search_paths();

        for path in candidates {
            if path.exists() {
                let config = Self::load(&path)?;
                return Ok((config, path));
            }
        }

        Err(anyhow!(
            "No configuration file found. Searched:\n  - GHOSTSNAP_CONFIG env var\n  - /etc/ghostsnap/jobs.toml\n  - ./ghostsnap.toml\n\nUse --config to specify a config file."
        ))
    }

    /// Get the list of paths to search for configuration.
    pub fn config_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Environment variable
        if let Ok(env_path) = std::env::var("GHOSTSNAP_CONFIG") {
            paths.push(PathBuf::from(env_path));
        }

        // System config
        paths.push(PathBuf::from("/etc/ghostsnap/jobs.toml"));

        // Local config
        paths.push(PathBuf::from("./ghostsnap.toml"));

        paths
    }

    /// Get a job by name.
    pub fn get_job(&self, name: &str) -> Option<&Job> {
        self.jobs.get(name)
    }
}

/// A resolved job with defaults applied.
#[derive(Debug, Clone)]
pub struct ResolvedJob {
    pub name: String,
    pub repository: String,
    pub password_env: Option<String>,
    pub password_file: Option<PathBuf>,
    pub paths: Vec<PathBuf>,
    pub tags: Vec<String>,
    pub exclude: Vec<String>,
    pub exclude_if_present: Vec<String>,
    pub hostname: Option<String>,
    pub one_file_system: bool,

    // Hooks
    pub pre_hook: Option<String>,
    pub post_hook: Option<String>,
    pub pre_hook_timeout: Duration,
    pub post_hook_timeout: Duration,
    pub shell: String,
    pub working_directory: Option<PathBuf>,

    // Retention
    pub keep_last: Option<u32>,
    pub keep_hourly: Option<u32>,
    pub keep_daily: Option<u32>,
    pub keep_weekly: Option<u32>,
    pub keep_monthly: Option<u32>,
    pub keep_yearly: Option<u32>,
    pub prune: bool,

    // Safety
    pub require_paths_exist: bool,
    pub stop_on_pre_hook_failure: bool,
    pub dry_run: bool,
}

impl ResolvedJob {
    /// Resolve a job by merging with defaults.
    pub fn resolve(name: &str, job: &Job, defaults: &JobDefaults) -> Result<Self> {
        let repository = job
            .repository
            .clone()
            .or_else(|| defaults.repository.clone())
            .ok_or_else(|| anyhow!("Job '{}' has no repository configured", name))?;

        let password_env = job.password_env.clone().or_else(|| defaults.password_env.clone());
        let password_file = job.password_file.clone().or_else(|| defaults.password_file.clone());

        // Combine paths and extra_paths
        let mut paths: Vec<PathBuf> = job.paths.iter().map(PathBuf::from).collect();
        paths.extend(job.extra_paths.iter().map(PathBuf::from));

        let shell = job
            .shell
            .clone()
            .or_else(|| defaults.shell.clone())
            .unwrap_or_else(|| "/bin/sh".to_string());

        let pre_hook_timeout = parse_duration(&job.pre_hook_timeout.clone().unwrap_or_else(|| "5m".to_string()))?;
        let post_hook_timeout = parse_duration(&job.post_hook_timeout.clone().unwrap_or_else(|| "5m".to_string()))?;

        Ok(Self {
            name: name.to_string(),
            repository,
            password_env,
            password_file,
            paths,
            tags: job.tags.clone(),
            exclude: job.exclude.clone(),
            exclude_if_present: job.exclude_if_present.clone(),
            hostname: job.hostname.clone(),
            one_file_system: job.one_file_system,
            pre_hook: job.pre_hook.clone(),
            post_hook: job.post_hook.clone(),
            pre_hook_timeout,
            post_hook_timeout,
            shell,
            working_directory: job.working_directory.clone(),
            keep_last: job.keep_last,
            keep_hourly: job.keep_hourly,
            keep_daily: job.keep_daily,
            keep_weekly: job.keep_weekly,
            keep_monthly: job.keep_monthly,
            keep_yearly: job.keep_yearly,
            prune: job.prune,
            require_paths_exist: job.require_paths_exist,
            stop_on_pre_hook_failure: job.stop_on_pre_hook_failure,
            dry_run: job.dry_run,
        })
    }

    /// Check if any retention policy is configured.
    pub fn has_retention_policy(&self) -> bool {
        self.keep_last.is_some()
            || self.keep_hourly.is_some()
            || self.keep_daily.is_some()
            || self.keep_weekly.is_some()
            || self.keep_monthly.is_some()
            || self.keep_yearly.is_some()
    }

    /// Resolve the password from environment variable or file.
    pub fn resolve_password(&self) -> Result<String> {
        // Try environment variable first
        if let Some(env_var) = &self.password_env
            && let Ok(password) = std::env::var(env_var)
        {
            return Ok(password);
        }

        // Try password file
        if let Some(file_path) = &self.password_file {
            let password = fs::read_to_string(file_path)
                .with_context(|| format!("Failed to read password file: {}", file_path.display()))?
                .trim()
                .to_string();
            return Ok(password);
        }

        Err(anyhow!(
            "No password configured. Set password_env or password_file in job config."
        ))
    }
}

/// Parse a duration string like "5m", "30s", "1h".
fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Duration::from_secs(300)); // Default 5 minutes
    }

    let (num_str, unit) = if let Some(stripped) = s.strip_suffix('s') {
        (stripped, "s")
    } else if let Some(stripped) = s.strip_suffix('m') {
        (stripped, "m")
    } else if let Some(stripped) = s.strip_suffix('h') {
        (stripped, "h")
    } else {
        // Assume seconds if no unit
        (s, "s")
    };

    let num: u64 = num_str
        .parse()
        .with_context(|| format!("Invalid duration number: {}", num_str))?;

    let seconds = match unit {
        "s" => num,
        "m" => num * 60,
        "h" => num * 3600,
        _ => num,
    };

    Ok(Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("120").unwrap(), Duration::from_secs(120));
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
            version = 1

            [defaults]
            repository = "s3:default-bucket/backups"
            password_env = "BACKUP_PASSWORD"

            [jobs.test-job]
            paths = ["/tmp/test"]
            tags = ["test"]
            keep_daily = 7
        "#;

        let config: JobConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.defaults.repository, Some("s3:default-bucket/backups".to_string()));
        assert!(config.jobs.contains_key("test-job"));

        let job = config.jobs.get("test-job").unwrap();
        assert_eq!(job.paths, vec!["/tmp/test"]);
        assert_eq!(job.keep_daily, Some(7));
    }

    #[test]
    fn test_resolve_job() {
        let defaults = JobDefaults {
            repository: Some("s3:default/repo".to_string()),
            password_env: Some("DEFAULT_PASSWORD".to_string()),
            password_file: None,
            shell: None,
        };

        let job = Job {
            repository: None,
            password_env: None,
            password_file: None,
            paths: vec!["/data".to_string()],
            extra_paths: vec!["/staging".to_string()],
            tags: vec!["test".to_string()],
            exclude: vec![],
            exclude_if_present: vec![],
            hostname: None,
            one_file_system: false,
            pre_hook: None,
            post_hook: None,
            pre_hook_timeout: None,
            post_hook_timeout: None,
            shell: None,
            working_directory: None,
            keep_last: Some(10),
            keep_hourly: None,
            keep_daily: Some(7),
            keep_weekly: None,
            keep_monthly: None,
            keep_yearly: None,
            prune: true,
            require_paths_exist: true,
            stop_on_pre_hook_failure: true,
            dry_run: false,
        };

        let resolved = ResolvedJob::resolve("test", &job, &defaults).unwrap();
        assert_eq!(resolved.repository, "s3:default/repo");
        assert_eq!(resolved.password_env, Some("DEFAULT_PASSWORD".to_string()));
        assert_eq!(resolved.paths.len(), 2);
        assert!(resolved.has_retention_policy());
    }
}
