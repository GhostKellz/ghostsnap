//! Tests for config-driven job commands.
//!
//! These tests verify that the job system correctly parses configs,
//! executes hooks, runs backups, and applies retention policies.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

/// Get the path to the ghostsnap binary.
fn ghostsnap_bin() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // Remove test binary name
    path.pop(); // Remove deps directory
    path.push("ghostsnap");
    if !path.exists() {
        let status = Command::new("cargo")
            .args(["build", "--bin", "ghostsnap"])
            .status()
            .expect("Failed to build ghostsnap binary");
        assert!(status.success(), "Failed to build ghostsnap");
    }
    path
}

/// Run ghostsnap with password via environment.
fn run_ghostsnap_with_password(args: &[&str], password: &str) -> (bool, String, String) {
    let output = Command::new(ghostsnap_bin())
        .args(args)
        .env("GHOSTSNAP_PASSWORD", password)
        .output()
        .expect("Failed to execute ghostsnap");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

/// Create a minimal job config file.
fn create_job_config(path: &std::path::Path, repo_path: &str, source_paths: &[&str], password_file: &str) -> String {
    let paths_toml: String = source_paths
        .iter()
        .map(|p| format!("\"{}\"", p))
        .collect::<Vec<_>>()
        .join(", ");

    let config = format!(
        r#"version = 1

[defaults]
password_file = "{}"

[jobs.test-job]
repository = "{}"
paths = [{}]
tags = ["test", "job"]
exclude = ["*.tmp", "*/cache/*"]

keep_daily = 7
prune = true
"#,
        password_file, repo_path, paths_toml
    );

    fs::write(path, &config).unwrap();
    config
}

/// Create a job config with hooks.
fn create_job_config_with_hooks(
    path: &std::path::Path,
    repo_path: &str,
    source_paths: &[&str],
    password_file: &str,
    pre_hook: &str,
    post_hook: &str,
) -> String {
    let paths_toml: String = source_paths
        .iter()
        .map(|p| format!("\"{}\"", p))
        .collect::<Vec<_>>()
        .join(", ");

    let config = format!(
        r#"version = 1

[defaults]
password_file = "{}"

[jobs.test-job]
repository = "{}"
paths = [{}]
tags = ["test", "hooks"]

pre_hook = """
{}
"""

post_hook = """
{}
"""

pre_hook_timeout = "30s"
post_hook_timeout = "30s"
"#,
        password_file, repo_path, paths_toml, pre_hook, post_hook
    );

    fs::write(path, &config).unwrap();
    config
}

// === Job List Tests ===

#[test]
fn test_job_list_command() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    create_job_config(
        &config_path,
        repo_path.to_str().unwrap(),
        &[source_path.to_str().unwrap()],
        password_file.to_str().unwrap(),
    );

    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &["job", "--config", config_path.to_str().unwrap(), "list"],
        "test-password",
    );

    assert!(success, "job list should succeed: {}", stderr);
    assert!(stdout.contains("test-job"), "Should list test-job: {}", stdout);
    assert!(stdout.contains("Jobs:"), "Should show Jobs header: {}", stdout);
}

#[test]
fn test_job_list_empty_config() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("empty.toml");

    fs::write(&config_path, "version = 1\n").unwrap();

    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &["job", "--config", config_path.to_str().unwrap(), "list"],
        "test-password",
    );

    assert!(success, "job list with empty config should succeed: {}", stderr);
    assert!(
        stdout.contains("No jobs configured"),
        "Should indicate no jobs: {}",
        stdout
    );
}

// === Job Show Tests ===

#[test]
fn test_job_show_command() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    create_job_config(
        &config_path,
        repo_path.to_str().unwrap(),
        &[source_path.to_str().unwrap()],
        password_file.to_str().unwrap(),
    );

    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "show",
            "test-job",
        ],
        "test-password",
    );

    assert!(success, "job show should succeed: {}", stderr);
    assert!(stdout.contains("Job: test-job"), "Should show job name: {}", stdout);
    assert!(stdout.contains("Repository:"), "Should show repository: {}", stdout);
    assert!(stdout.contains("Paths:"), "Should show paths: {}", stdout);
    assert!(stdout.contains("Tags:"), "Should show tags: {}", stdout);
    assert!(stdout.contains("Retention:"), "Should show retention: {}", stdout);
}

#[test]
fn test_job_show_not_found() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");

    fs::write(&config_path, "version = 1\n").unwrap();

    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "show",
            "nonexistent",
        ],
        "test-password",
    );

    assert!(!success, "job show nonexistent should fail");
    assert!(
        stderr.contains("not found") || stderr.contains("nonexistent"),
        "Should mention job not found: {}",
        stderr
    );
}

// === Job Validate Tests ===

#[test]
fn test_job_validate_success() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    create_job_config(
        &config_path,
        repo_path.to_str().unwrap(),
        &[source_path.to_str().unwrap()],
        password_file.to_str().unwrap(),
    );

    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "validate",
            "test-job",
        ],
        "test-password",
    );

    assert!(success, "job validate should succeed: {}", stderr);
    assert!(
        stdout.contains("Validation passed"),
        "Should indicate validation passed: {}",
        stdout
    );
}

#[test]
fn test_job_validate_missing_paths() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let password_file = temp.path().join("password");

    fs::write(&password_file, "test-password").unwrap();

    // Create config with non-existent paths
    create_job_config(
        &config_path,
        repo_path.to_str().unwrap(),
        &["/nonexistent/path/that/does/not/exist"],
        password_file.to_str().unwrap(),
    );

    let (success, stdout, _stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "validate",
            "test-job",
        ],
        "test-password",
    );

    // Validation should fail due to missing paths
    assert!(!success, "job validate with missing paths should fail");
    assert!(
        stdout.contains("ERROR") || stdout.contains("does not exist"),
        "Should indicate path error: {}",
        stdout
    );
}

// === Job Run Tests ===

#[test]
fn test_job_run_success() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create test file
    let test_file = source_path.join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Job test content").unwrap();

    create_job_config(
        &config_path,
        repo_path.to_str().unwrap(),
        &[source_path.to_str().unwrap()],
        password_file.to_str().unwrap(),
    );

    // First init the repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Run the job
    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "test-job",
        ],
        "test-password",
    );

    assert!(success, "job run should succeed: {}\n{}", stderr, stdout);
    assert!(stdout.contains("Job: test-job"), "Should show job name: {}", stdout);
    assert!(stdout.contains("Backup: OK"), "Should show backup success: {}", stdout);
    assert!(stdout.contains("Snapshot:"), "Should show snapshot ID: {}", stdout);
}

#[test]
fn test_job_run_dry_run() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create test file
    let test_file = source_path.join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Dry run test content").unwrap();

    create_job_config(
        &config_path,
        repo_path.to_str().unwrap(),
        &[source_path.to_str().unwrap()],
        password_file.to_str().unwrap(),
    );

    // Init the repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Run with --dry-run
    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "test-job",
            "--dry-run",
        ],
        "test-password",
    );

    assert!(success, "job run --dry-run should succeed: {}", stderr);
    assert!(
        stdout.contains("dry run"),
        "Should indicate dry run: {}",
        stdout
    );

    // Verify no snapshot was created
    let (success, list_stdout, _) = run_ghostsnap_with_password(
        &["--repo", repo_path.to_str().unwrap(), "snapshots"],
        "test-password",
    );
    assert!(success, "snapshots should succeed");
    assert!(
        list_stdout.contains("No snapshots") || !list_stdout.lines().any(|l| {
            let trimmed = l.trim();
            trimmed.len() >= 8
                && trimmed
                    .split_whitespace()
                    .next()
                    .map(|w| w.chars().all(|c| c.is_ascii_hexdigit()))
                    .unwrap_or(false)
        }),
        "Should have no snapshots after dry run: {}",
        list_stdout
    );
}

#[test]
fn test_job_run_with_hooks() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");
    let hook_marker = temp.path().join("hook_ran");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create test file
    let test_file = source_path.join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Hook test content").unwrap();

    // Create config with hooks that create marker files
    let pre_hook = format!("touch {}", hook_marker.to_str().unwrap());
    let post_hook = "echo 'post-hook completed'";

    create_job_config_with_hooks(
        &config_path,
        repo_path.to_str().unwrap(),
        &[source_path.to_str().unwrap()],
        password_file.to_str().unwrap(),
        &pre_hook,
        post_hook,
    );

    // Init the repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Run the job
    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "test-job",
        ],
        "test-password",
    );

    assert!(success, "job run with hooks should succeed: {}\n{}", stderr, stdout);
    assert!(
        stdout.contains("Pre-hook: OK"),
        "Should show pre-hook success: {}",
        stdout
    );
    assert!(
        stdout.contains("Post-hook: OK"),
        "Should show post-hook success: {}",
        stdout
    );
    assert!(hook_marker.exists(), "Pre-hook should have created marker file");
}

#[test]
fn test_job_run_pre_hook_failure() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create test file
    let test_file = source_path.join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Failed hook test").unwrap();

    // Create config with failing pre-hook
    create_job_config_with_hooks(
        &config_path,
        repo_path.to_str().unwrap(),
        &[source_path.to_str().unwrap()],
        password_file.to_str().unwrap(),
        "exit 1",  // Failing pre-hook
        "echo 'post'",
    );

    // Init the repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Run the job - should fail due to pre-hook
    let (success, stdout, _stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "test-job",
        ],
        "test-password",
    );

    assert!(!success, "job run with failed pre-hook should fail");
    assert!(
        stdout.contains("Pre-hook: FAILED") || stdout.contains("Pre-hook failed"),
        "Should indicate pre-hook failure: {}",
        stdout
    );
}

#[test]
fn test_job_run_with_retention() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create test file
    let test_file = source_path.join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Retention test content").unwrap();

    create_job_config(
        &config_path,
        repo_path.to_str().unwrap(),
        &[source_path.to_str().unwrap()],
        password_file.to_str().unwrap(),
    );

    // Init the repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Run the job
    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "test-job",
        ],
        "test-password",
    );

    assert!(success, "job run should succeed: {}\n{}", stderr, stdout);
    // Check that forget and prune ran (the job config has keep_daily=7 and prune=true)
    assert!(
        stdout.contains("Forget: OK"),
        "Should show forget success: {}",
        stdout
    );
    assert!(
        stdout.contains("Prune: OK"),
        "Should show prune success: {}",
        stdout
    );
}

// === Job Exclude Parity Tests ===

#[test]
fn test_job_excludes_glob_patterns() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::create_dir_all(source_path.join("cache")).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create files that should be included
    let included_file = source_path.join("included.txt");
    File::create(&included_file)
        .unwrap()
        .write_all(b"included")
        .unwrap();

    // Create files that should be excluded
    let excluded_tmp = source_path.join("excluded.tmp");
    File::create(&excluded_tmp)
        .unwrap()
        .write_all(b"excluded tmp")
        .unwrap();

    let excluded_cache = source_path.join("cache").join("cached.txt");
    File::create(&excluded_cache)
        .unwrap()
        .write_all(b"excluded cache")
        .unwrap();

    create_job_config(
        &config_path,
        repo_path.to_str().unwrap(),
        &[source_path.to_str().unwrap()],
        password_file.to_str().unwrap(),
    );

    // Init the repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Run the job
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "test-job",
        ],
        "test-password",
    );
    assert!(success, "job run should succeed: {}", stderr);

    // List files in the snapshot
    let (success, stdout, _) = run_ghostsnap_with_password(
        &["--repo", repo_path.to_str().unwrap(), "snapshots"],
        "test-password",
    );
    assert!(success, "snapshots should succeed");

    // Get snapshot ID
    let snapshot_id = stdout
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.len() >= 8 {
                let first_word = trimmed.split_whitespace().next()?;
                if first_word.len() == 8 && first_word.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(first_word.to_string());
                }
            }
            None
        })
        .expect("Should have snapshot");

    // List files in snapshot
    let (success, ls_stdout, _) = run_ghostsnap_with_password(
        &[
            "--repo",
            repo_path.to_str().unwrap(),
            "ls",
            &snapshot_id,
            "-r",
        ],
        "test-password",
    );
    assert!(success, "ls should succeed");

    // Verify included file is present
    assert!(
        ls_stdout.contains("included.txt"),
        "Should include included.txt: {}",
        ls_stdout
    );

    // Verify excluded files are not present
    assert!(
        !ls_stdout.contains("excluded.tmp"),
        "Should exclude *.tmp files: {}",
        ls_stdout
    );
    assert!(
        !ls_stdout.contains("cached.txt"),
        "Should exclude */cache/* files: {}",
        ls_stdout
    );
}

// === Job/Backup Parity Tests ===

/// Verifies that job backup produces the same files as direct backup command
/// when given the same paths and exclude patterns.
#[test]
fn test_job_backup_parity_with_excludes() {
    let temp = tempdir().unwrap();
    let job_repo = temp.path().join("job-repo");
    let backup_repo = temp.path().join("backup-repo");
    let source_path = temp.path().join("source");
    let config_path = temp.path().join("jobs.toml");
    let password_file = temp.path().join("password");

    // Create directory structure
    fs::create_dir_all(&source_path).unwrap();
    fs::create_dir_all(source_path.join("cache")).unwrap();
    fs::create_dir_all(source_path.join("logs")).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create test files
    File::create(source_path.join("app.txt"))
        .unwrap()
        .write_all(b"application data")
        .unwrap();
    File::create(source_path.join("config.json"))
        .unwrap()
        .write_all(b"{}")
        .unwrap();
    File::create(source_path.join("data.tmp"))
        .unwrap()
        .write_all(b"temp file")
        .unwrap();
    File::create(source_path.join("cache").join("cached.dat"))
        .unwrap()
        .write_all(b"cached")
        .unwrap();
    File::create(source_path.join("logs").join("app.log"))
        .unwrap()
        .write_all(b"log data")
        .unwrap();

    // Create job config with excludes
    let config = format!(
        r#"version = 1

[defaults]
password_file = "{}"

[jobs.parity-test]
repository = "{}"
paths = ["{}"]
exclude = ["*.tmp", "*.log", "*/cache/*"]
"#,
        password_file.to_str().unwrap(),
        job_repo.to_str().unwrap(),
        source_path.to_str().unwrap()
    );
    fs::write(&config_path, &config).unwrap();

    // Init both repos
    let (s1, _, e1) = run_ghostsnap_with_password(&["init", job_repo.to_str().unwrap()], "test-password");
    let (s2, _, e2) = run_ghostsnap_with_password(&["init", backup_repo.to_str().unwrap()], "test-password");
    assert!(s1, "Init job repo: {}", e1);
    assert!(s2, "Init backup repo: {}", e2);

    // Run job backup
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "parity-test",
        ],
        "test-password",
    );
    assert!(success, "Job backup should succeed: {}", stderr);

    // Run direct backup with same excludes
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &[
            "--repo",
            backup_repo.to_str().unwrap(),
            "backup",
            source_path.to_str().unwrap(),
            "--exclude",
            "*.tmp",
            "--exclude",
            "*.log",
            "--exclude",
            "*/cache/*",
        ],
        "test-password",
    );
    assert!(success, "Direct backup should succeed: {}", stderr);

    // Get snapshot IDs
    let get_snapshot_id = |repo: &str| -> String {
        let (_, stdout, _) = run_ghostsnap_with_password(
            &["--repo", repo, "snapshots"],
            "test-password",
        );
        stdout
            .lines()
            .find_map(|line| {
                let trimmed = line.trim();
                if trimmed.len() >= 8 {
                    let first_word = trimmed.split_whitespace().next()?;
                    if first_word.len() == 8 && first_word.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Some(first_word.to_string());
                    }
                }
                None
            })
            .expect("Should have snapshot")
    };

    let job_snapshot = get_snapshot_id(job_repo.to_str().unwrap());
    let backup_snapshot = get_snapshot_id(backup_repo.to_str().unwrap());

    // List files in both snapshots
    let list_files = |repo: &str, snapshot: &str| -> Vec<String> {
        let (_, stdout, _) = run_ghostsnap_with_password(
            &["--repo", repo, "ls", snapshot, "-r"],
            "test-password",
        );
        stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };

    let job_files = list_files(job_repo.to_str().unwrap(), &job_snapshot);
    let backup_files = list_files(backup_repo.to_str().unwrap(), &backup_snapshot);

    // Verify expected files are included
    assert!(
        job_files.iter().any(|f| f.contains("app.txt")),
        "Should include app.txt\nJob files: {:?}",
        job_files
    );
    assert!(
        job_files.iter().any(|f| f.contains("config.json")),
        "Should include config.json\nJob files: {:?}",
        job_files
    );

    // Verify excluded files are not present (check for the actual file names, not directories)
    assert!(
        !job_files.iter().any(|f| f.ends_with(".tmp")),
        "Should not include .tmp files\nJob files: {:?}",
        job_files
    );
    assert!(
        !job_files.iter().any(|f| f.ends_with(".log")),
        "Should not include .log files\nJob files: {:?}",
        job_files
    );
    // Check for cached.dat specifically since "cache" directory may exist as empty dir
    assert!(
        !job_files.iter().any(|f| f.contains("cached.dat")),
        "Should not include cached.dat file\nJob files: {:?}",
        job_files
    );

    // Both should have similar file counts (allowing for empty dirs which may or may not be included)
    let job_file_count = job_files.iter().filter(|f| !f.ends_with('/') && f.contains('.')).count();
    let backup_file_count = backup_files.iter().filter(|f| !f.ends_with('/') && f.contains('.')).count();
    assert_eq!(
        job_file_count,
        backup_file_count,
        "Job and backup should have same regular file count\nJob: {:?}\nBackup: {:?}",
        job_files,
        backup_files
    );
}

/// Test that one_file_system option works in job execution.
#[test]
fn test_job_one_file_system_option() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create test file
    File::create(source_path.join("local.txt"))
        .unwrap()
        .write_all(b"local file")
        .unwrap();

    // Create config with one_file_system = true
    let config = format!(
        r#"version = 1

[defaults]
password_file = "{}"

[jobs.one-fs-test]
repository = "{}"
paths = ["{}"]
one_file_system = true
"#,
        password_file.to_str().unwrap(),
        repo_path.to_str().unwrap(),
        source_path.to_str().unwrap()
    );
    fs::write(&config_path, &config).unwrap();

    // Init repo
    let (success, _, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Run job
    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "one-fs-test",
        ],
        "test-password",
    );

    assert!(success, "Job with one_file_system should succeed: {}\n{}", stderr, stdout);
    assert!(stdout.contains("Backup: OK"), "Backup should succeed: {}", stdout);
}

/// Test that exclude_if_present option works in job execution.
#[test]
fn test_job_exclude_if_present_option() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    // Create directories
    fs::create_dir_all(&source_path).unwrap();
    fs::create_dir_all(source_path.join("included")).unwrap();
    fs::create_dir_all(source_path.join("excluded")).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create test files
    File::create(source_path.join("included").join("data.txt"))
        .unwrap()
        .write_all(b"included data")
        .unwrap();
    File::create(source_path.join("excluded").join("data.txt"))
        .unwrap()
        .write_all(b"excluded data")
        .unwrap();

    // Create .nobackup marker in excluded directory
    File::create(source_path.join("excluded").join(".nobackup"))
        .unwrap()
        .write_all(b"")
        .unwrap();

    // Create config with exclude_if_present
    let config = format!(
        r#"version = 1

[defaults]
password_file = "{}"

[jobs.marker-test]
repository = "{}"
paths = ["{}"]
exclude_if_present = [".nobackup"]
"#,
        password_file.to_str().unwrap(),
        repo_path.to_str().unwrap(),
        source_path.to_str().unwrap()
    );
    fs::write(&config_path, &config).unwrap();

    // Init repo
    let (success, _, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Run job
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "marker-test",
        ],
        "test-password",
    );
    assert!(success, "Job should succeed: {}", stderr);

    // Get snapshot and list files
    let (_, stdout, _) = run_ghostsnap_with_password(
        &["--repo", repo_path.to_str().unwrap(), "snapshots"],
        "test-password",
    );
    let snapshot_id = stdout
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.len() >= 8 {
                let first_word = trimmed.split_whitespace().next()?;
                if first_word.len() == 8 && first_word.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(first_word.to_string());
                }
            }
            None
        })
        .expect("Should have snapshot");

    let (_, ls_stdout, _) = run_ghostsnap_with_password(
        &["--repo", repo_path.to_str().unwrap(), "ls", &snapshot_id, "-r"],
        "test-password",
    );

    // Verify included directory's file is present
    assert!(
        ls_stdout.contains("included"),
        "Should include 'included' directory: {}",
        ls_stdout
    );

    // Verify excluded directory (with .nobackup marker) is not present
    assert!(
        !ls_stdout.contains("excluded/data.txt"),
        "Should not include files from excluded directory: {}",
        ls_stdout
    );
}

// === Job Run All Tests ===

#[test]
fn test_job_run_all() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("jobs.toml");
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let password_file = temp.path().join("password");

    fs::create_dir_all(&source_path).unwrap();
    fs::write(&password_file, "test-password").unwrap();

    // Create test file
    let test_file = source_path.join("test.txt");
    File::create(&test_file)
        .unwrap()
        .write_all(b"Run all test")
        .unwrap();

    // Create config with multiple jobs
    let paths_str = format!("\"{}\"", source_path.to_str().unwrap());
    let config = format!(
        r#"version = 1

[defaults]
password_file = "{}"

[jobs.job-a]
repository = "{}"
paths = [{}]
tags = ["job-a"]

[jobs.job-b]
repository = "{}"
paths = [{}]
tags = ["job-b"]
"#,
        password_file.to_str().unwrap(),
        repo_path.to_str().unwrap(),
        paths_str,
        repo_path.to_str().unwrap(),
        paths_str
    );
    fs::write(&config_path, &config).unwrap();

    // Init the repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Run all jobs
    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &[
            "job",
            "--config",
            config_path.to_str().unwrap(),
            "run",
            "--all",
        ],
        "test-password",
    );

    assert!(success, "job run --all should succeed: {}\n{}", stderr, stdout);
    assert!(
        stdout.contains("Running 2 jobs"),
        "Should indicate running 2 jobs: {}",
        stdout
    );
    assert!(
        stdout.contains("Job: job-a") && stdout.contains("Job: job-b"),
        "Should show both job names: {}",
        stdout
    );
    assert!(
        stdout.contains("2 succeeded"),
        "Should show 2 succeeded: {}",
        stdout
    );
}
