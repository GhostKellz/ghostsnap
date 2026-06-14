//! Binary-level CLI smoke tests.
//!
//! These tests invoke the compiled `ghostsnap` binary directly to verify
//! that CLI parsing works as documented. This catches drift between
//! documentation and actual CLI behavior.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

/// Get the path to the ghostsnap binary.
fn ghostsnap_bin() -> PathBuf {
    // When running `cargo test`, the binary is in target/debug
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // Remove test binary name
    path.pop(); // Remove deps directory
    path.push("ghostsnap");
    if !path.exists() {
        // Try building it
        let status = Command::new("cargo")
            .args(["build", "--bin", "ghostsnap"])
            .status()
            .expect("Failed to build ghostsnap binary");
        assert!(status.success(), "Failed to build ghostsnap");
    }
    path
}

/// Run ghostsnap with given arguments and return (success, stdout, stderr).
fn run_ghostsnap(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(ghostsnap_bin())
        .args(args)
        .output()
        .expect("Failed to execute ghostsnap");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
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

#[test]
fn test_cli_help() {
    let (success, stdout, _stderr) = run_ghostsnap(&["--help"]);
    assert!(success, "ghostsnap --help should succeed");
    assert!(stdout.contains("ghostsnap"), "Help should mention ghostsnap");
    assert!(stdout.contains("backup"), "Help should list backup command");
    assert!(stdout.contains("restore"), "Help should list restore command");
    assert!(stdout.contains("--repo"), "Help should document --repo flag");
}

#[test]
fn test_cli_init_help() {
    let (success, stdout, _stderr) = run_ghostsnap(&["init", "--help"]);
    assert!(success, "ghostsnap init --help should succeed");
    assert!(stdout.contains("Initialize"), "Init help should describe initialization");
    assert!(stdout.contains("--backend"), "Init should document --backend flag");
}

#[test]
fn test_cli_backup_help() {
    let (success, stdout, _stderr) = run_ghostsnap(&["backup", "--help"]);
    assert!(success, "ghostsnap backup --help should succeed");
    assert!(stdout.contains("backup"), "Backup help should mention backup");
    assert!(stdout.contains("--tag"), "Backup should document --tag flag");
    assert!(stdout.contains("--exclude"), "Backup should document --exclude flag");
}

#[test]
fn test_cli_repo_before_subcommand() {
    // Test that --repo must come before the subcommand
    // This tests the documented syntax: ghostsnap --repo /path backup ...
    let temp = tempdir().unwrap();
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    fs::create_dir_all(&source_path).unwrap();

    // First init the repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Test correct syntax: --repo before subcommand
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &[
            "--repo",
            repo_path.to_str().unwrap(),
            "backup",
            source_path.to_str().unwrap(),
        ],
        "test-password",
    );
    assert!(success, "Correct CLI syntax should succeed: {}", stderr);
}

#[test]
fn test_cli_init_local_repo() {
    let temp = tempdir().unwrap();
    let repo_path = temp.path().join("test-repo");

    // Test: ghostsnap init /path
    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);
    assert!(
        stdout.contains("Successfully initialized") || stdout.contains("initialized"),
        "Should confirm initialization: {}",
        stdout
    );

    // Verify repo was created
    assert!(repo_path.join("config").exists(), "Config should exist");
    assert!(repo_path.join("keys").is_dir(), "Keys dir should exist");
}

#[test]
fn test_cli_snapshots_command() {
    let temp = tempdir().unwrap();
    let repo_path = temp.path().join("repo");

    // Init repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Test: ghostsnap --repo /path snapshots
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["--repo", repo_path.to_str().unwrap(), "snapshots"],
        "test-password",
    );
    assert!(success, "Snapshots command should succeed: {}", stderr);
}

#[test]
fn test_cli_check_command() {
    let temp = tempdir().unwrap();
    let repo_path = temp.path().join("repo");

    // Init repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Test: ghostsnap --repo /path check
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["--repo", repo_path.to_str().unwrap(), "check"],
        "test-password",
    );
    assert!(success, "Check command should succeed: {}", stderr);
}

#[test]
fn test_cli_stats_command() {
    let temp = tempdir().unwrap();
    let repo_path = temp.path().join("repo");

    // Init repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Test: ghostsnap --repo /path stats
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["--repo", repo_path.to_str().unwrap(), "stats"],
        "test-password",
    );
    assert!(success, "Stats command should succeed: {}", stderr);
}

#[test]
fn test_cli_backup_and_restore_workflow() {
    let temp = tempdir().unwrap();
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    let restore_path = temp.path().join("restore");
    fs::create_dir_all(&source_path).unwrap();

    // Create test file
    let test_file = source_path.join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Hello, Ghostsnap CLI test!").unwrap();

    // Init repo
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["init", repo_path.to_str().unwrap()],
        "test-password",
    );
    assert!(success, "Init should succeed: {}", stderr);

    // Backup: ghostsnap --repo /path backup /source
    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &[
            "--repo",
            repo_path.to_str().unwrap(),
            "backup",
            source_path.to_str().unwrap(),
        ],
        "test-password",
    );
    assert!(success, "Backup should succeed: {}", stderr);
    assert!(
        stdout.contains("Snapshot:") || stdout.contains("Backup completed"),
        "Should confirm backup: {}",
        stdout
    );

    // Get snapshot ID from snapshots list
    let (success, stdout, stderr) = run_ghostsnap_with_password(
        &["--repo", repo_path.to_str().unwrap(), "snapshots"],
        "test-password",
    );
    assert!(success, "Snapshots should succeed: {}", stderr);

    // Extract first snapshot ID - look for 8-char hex string at start of line
    let snapshot_id = stdout
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            // Snapshot IDs are 8-char hex strings
            if trimmed.len() >= 8 {
                let first_word = trimmed.split_whitespace().next()?;
                if first_word.len() == 8 && first_word.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(first_word.to_string());
                }
            }
            None
        })
        .expect("Should have at least one snapshot");
    let snapshot_id = snapshot_id.as_str();

    // Restore: ghostsnap --repo /path restore <id> --target /restore
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &[
            "--repo",
            repo_path.to_str().unwrap(),
            "restore",
            snapshot_id,
            "--target",
            restore_path.to_str().unwrap(),
        ],
        "test-password",
    );
    assert!(success, "Restore should succeed: {}", stderr);

    // Verify restored file exists
    let restored_file = restore_path.join("test.txt");
    assert!(
        restored_file.exists(),
        "Restored file should exist at {:?}",
        restored_file
    );
}

#[test]
fn test_cli_forget_and_prune() {
    let temp = tempdir().unwrap();
    let repo_path = temp.path().join("repo");
    let source_path = temp.path().join("source");
    fs::create_dir_all(&source_path).unwrap();

    // Create test file
    let test_file = source_path.join("data.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Some data for backup").unwrap();

    // Init and backup
    let _ = run_ghostsnap_with_password(&["init", repo_path.to_str().unwrap()], "test-password");
    let _ = run_ghostsnap_with_password(
        &[
            "--repo",
            repo_path.to_str().unwrap(),
            "backup",
            source_path.to_str().unwrap(),
        ],
        "test-password",
    );

    // Test forget: ghostsnap --repo /path forget --keep-last 1
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &[
            "--repo",
            repo_path.to_str().unwrap(),
            "forget",
            "--keep-last",
            "1",
        ],
        "test-password",
    );
    assert!(success, "Forget should succeed: {}", stderr);

    // Test prune: ghostsnap --repo /path prune
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &["--repo", repo_path.to_str().unwrap(), "prune"],
        "test-password",
    );
    assert!(success, "Prune should succeed: {}", stderr);
}

#[test]
fn test_cli_copy_between_repos() {
    let temp = tempdir().unwrap();
    let repo1_path = temp.path().join("repo1");
    let repo2_path = temp.path().join("repo2");
    let source_path = temp.path().join("source");
    fs::create_dir_all(&source_path).unwrap();

    // Create test file
    let test_file = source_path.join("copy-test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Data for copy test").unwrap();

    // Init both repos
    let _ = run_ghostsnap_with_password(&["init", repo1_path.to_str().unwrap()], "test-password");
    let _ = run_ghostsnap_with_password(&["init", repo2_path.to_str().unwrap()], "test-password");

    // Backup to repo1
    let _ = run_ghostsnap_with_password(
        &[
            "--repo",
            repo1_path.to_str().unwrap(),
            "backup",
            source_path.to_str().unwrap(),
        ],
        "test-password",
    );

    // Get snapshot ID - look for 8-char hex string
    let (_, stdout, _) = run_ghostsnap_with_password(
        &["--repo", repo1_path.to_str().unwrap(), "snapshots"],
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

    // Copy: ghostsnap --repo /repo1 copy --repo2 /repo2 --password2 <password> <snapshot-id>
    let (success, _stdout, stderr) = run_ghostsnap_with_password(
        &[
            "--repo",
            repo1_path.to_str().unwrap(),
            "copy",
            "--repo2",
            repo2_path.to_str().unwrap(),
            "--password2",
            "test-password",
            &snapshot_id,
        ],
        "test-password",
    );
    assert!(success, "Copy should succeed: {}", stderr);

    // Verify snapshot exists in repo2
    let (success, stdout, _) = run_ghostsnap_with_password(
        &["--repo", repo2_path.to_str().unwrap(), "snapshots"],
        "test-password",
    );
    assert!(success, "Snapshots in repo2 should succeed");
    assert!(
        stdout.contains(&snapshot_id),
        "Repo2 should contain copied snapshot"
    );
}

#[test]
fn test_cli_env_var_repo() {
    let temp = tempdir().unwrap();
    let repo_path = temp.path().join("repo");

    // Init repo first
    let _ = run_ghostsnap_with_password(&["init", repo_path.to_str().unwrap()], "test-password");

    // Test using GHOSTSNAP_REPO env var instead of --repo flag
    let output = Command::new(ghostsnap_bin())
        .args(["snapshots"])
        .env("GHOSTSNAP_REPO", repo_path.to_str().unwrap())
        .env("GHOSTSNAP_PASSWORD", "test-password")
        .output()
        .expect("Failed to execute ghostsnap");

    assert!(
        output.status.success(),
        "Should work with GHOSTSNAP_REPO env var"
    );
}
