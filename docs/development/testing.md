# Testing

Ghostsnap has comprehensive test coverage across unit, integration, and end-to-end tests.

## Running Tests

```bash
# All tests
cargo test

# Specific test
cargo test test_basic_backup_restore

# Tests with output
cargo test -- --nocapture

# Single-threaded (for debugging)
cargo test -- --test-threads=1
```

## Native S3 Integration Testing

Ghostsnap includes an opt-in native S3 integration test.

It is disabled by default so local development and CI remain stable without cloud credentials.

### Required Environment Variables

```bash
export GHOSTSNAP_TEST_S3=1
export GHOSTSNAP_TEST_S3_BUCKET=my-test-bucket
export GHOSTSNAP_TEST_S3_PREFIX=ghostsnap-test/manual-run
export GHOSTSNAP_TEST_S3_PASSWORD=test-password
```

Optional:

```bash
export GHOSTSNAP_TEST_S3_REGION=us-east-1
export GHOSTSNAP_TEST_S3_ENDPOINT=https://s3.wasabisys.com
```

### Run The S3 Test

```bash
cargo test --test s3_integration -- --nocapture
```

### What It Verifies

- repository initialization at `s3:bucket/prefix`
- encrypted object writes for config, keys, index, snapshots, trees, and packs
- repository reopen through the shared location model
- snapshot and tree reads from S3-backed storage

## Azure Integration Testing

Opt-in Azure Blob Storage integration test, disabled by default.

### Required Environment Variables

```bash
export GHOSTSNAP_TEST_AZURE=1
export GHOSTSNAP_TEST_AZURE_CONTAINER=my-test-container
export GHOSTSNAP_TEST_AZURE_PASSWORD=test-password
```

Azure credentials should be configured via `AZURE_STORAGE_ACCOUNT` and `AZURE_STORAGE_KEY` or Azure CLI login.

### Run The Azure Test

```bash
cargo test --test azure_integration -- --nocapture
```

## Rclone Integration Testing

Opt-in rclone integration test for testing any rclone-supported backend.

### Required Environment Variables

```bash
export GHOSTSNAP_TEST_RCLONE=1
export GHOSTSNAP_TEST_RCLONE_REMOTE=myremote  # rclone remote name
```

Optional:

```bash
export GHOSTSNAP_TEST_RCLONE_PATH=ghostsnap-test  # path within remote
export GHOSTSNAP_TEST_RCLONE_PASSWORD=test-password
```

### Prerequisites

1. Install rclone: https://rclone.org/install/
2. Configure a remote: `rclone config`
3. Verify it works: `rclone lsd myremote:`

### Run The Rclone Test

```bash
cargo test --test rclone_integration -- --nocapture
```

### What It Verifies

- repository initialization via rclone backend
- backup/restore roundtrip through rclone
- copy between local and rclone repositories
- listing operations with path prefixes

## CLI Binary Smoke Tests

Non-opt-in tests that invoke the compiled `ghostsnap` binary to verify CLI parsing.

### Run The Binary Tests

```bash
cargo test --test cli_binary
```

### What It Verifies

- Help output for all commands
- `--repo` flag position (must be before subcommand)
- Init, backup, restore, snapshots, check, stats, forget, prune, copy workflows
- Environment variable support (`GHOSTSNAP_REPO`, `GHOSTSNAP_PASSWORD`)

These tests catch documentation drift by testing the actual CLI behavior.

## Manual Release Validation Matrix

Before release or commit, validate these paths:

| Scenario | Command Shape | Expected Result |
|----------|---------------|-----------------|
| Local init | `ghostsnap --repo /backup/repo init` | Repository created locally |
| Local backup | `ghostsnap --repo /backup/repo backup /data` | Snapshot created |
| Local restore | `ghostsnap --repo /backup/repo restore <snapshot> --target /restore` | Data restored |
| S3 init | `ghostsnap --repo s3:<bucket>/<prefix> init --backend s3 --bucket <bucket> --prefix <prefix>` | Repository created in S3 |
| S3 backup | `ghostsnap --repo s3:<bucket>/<prefix> backup /data` | Snapshot created in S3 repository |
| S3 snapshots | `ghostsnap --repo s3:<bucket>/<prefix> snapshots` | Snapshot list returns expected entries |
| S3 check | `ghostsnap --repo s3:<bucket>/<prefix> check --read-data` | Repository passes integrity check |
| S3 stats | `ghostsnap --repo s3:<bucket>/<prefix> stats` | Sizes and counts are reported |
| Copy local->S3 | `ghostsnap --repo /backup/repo copy --repo2 s3:<bucket>/<prefix> <snapshot>` | Snapshot copied successfully |
| Provider compatibility | Repeat S3 init/backup/check against Wasabi or B2 S3-compatible endpoint with `--endpoint` | Same repository workflow succeeds |
| Arch + Btrfs host | Validate backup/restore/check on your real Btrfs layout | Metadata and restore expectations hold |

## Test Organization

### Unit Tests

Located alongside source code in `src/` directories:

```rust
// core/src/chunker.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_boundaries() {
        // ...
    }
}
```

Run unit tests:

```bash
cargo test --lib
```

### Integration Tests

Located in `tests/` directories:

```
cli/tests/
├── cli_binary.rs         # Binary-level CLI smoke tests
├── rclone_integration.rs # Opt-in rclone integration tests
└── common/
    └── mod.rs            # Test utilities

backends/tests/
└── azure_integration.rs  # Opt-in Azure integration tests
```

Run integration tests:

```bash
# CLI binary smoke tests (no external dependencies)
cargo test --test cli_binary

# Rclone integration tests (opt-in, requires rclone)
GHOSTSNAP_TEST_RCLONE=1 GHOSTSNAP_TEST_RCLONE_REMOTE=myremote cargo test --test rclone_integration

# Azure integration tests (opt-in, requires Azure credentials)
GHOSTSNAP_TEST_AZURE=1 GHOSTSNAP_TEST_AZURE_CONTAINER=mycontainer cargo test --test azure_integration
```

### Test Categories

| Category | Location | Description |
|----------|----------|-------------|
| Unit | `*/src/**/*.rs` | Individual functions |
| CLI Binary | `cli/tests/cli_binary.rs` | Binary invocation smoke tests |
| Rclone | `cli/tests/rclone_integration.rs` | Opt-in rclone integration |
| Azure | `backends/tests/azure_integration.rs` | Opt-in Azure integration |
| S3 | `backends/tests/s3_integration.rs` | Opt-in S3 integration |

## Test Utilities

### TestRepo

Helper for creating temporary repositories:

```rust
use crate::common::TestRepo;

#[tokio::test]
async fn test_backup() {
    let repo = TestRepo::new().await;

    // Create test files
    repo.create_file("test.txt", b"hello").await;

    // Run backup
    let snapshot_id = repo.backup("test.txt").await;

    // Verify
    assert!(repo.snapshot_exists(&snapshot_id).await);
}
```

### Test Files

Create various test scenarios:

```rust
// Regular file
repo.create_file("file.txt", b"content").await;

// Nested directory
repo.create_dir("a/b/c").await;

// Symlink
repo.create_symlink("link", "target").await;

// Sparse file
repo.create_sparse_file("sparse.bin", 1024 * 1024, vec![(0, 512)]).await;

// File with xattr
repo.create_file_with_xattr("file.txt", b"data", &[("user.test", b"value")]).await;
```

## Writing Tests

### Basic Test

```rust
#[tokio::test]
async fn test_feature() {
    // Setup
    let repo = TestRepo::new().await;
    repo.create_file("test.txt", b"hello").await;

    // Action
    let result = repo.backup(".").await;

    // Assert
    assert!(result.is_ok());
}
```

### Testing Error Conditions

```rust
#[tokio::test]
async fn test_invalid_repo() {
    let result = Repository::open("/nonexistent/path").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}
```

### Testing with Cleanup

```rust
#[tokio::test]
async fn test_with_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let repo_path = temp.path().join("repo");

    // Test runs here
    // temp directory auto-cleaned on drop
}
```

## Test Coverage

Generate coverage report:

```bash
# Install coverage tool
cargo install cargo-tarpaulin

# Generate report
cargo tarpaulin --out Html

# View report
open tarpaulin-report.html
```

## Debugging Tests

### Print Output

```bash
cargo test test_name -- --nocapture
```

### Single Test

```bash
cargo test test_specific_function -- --exact
```

### With Logging

```bash
RUST_LOG=debug cargo test test_name -- --nocapture
```

### GDB/LLDB

```bash
# Build tests with debug symbols
cargo test --no-run

# Find test binary
ls target/debug/deps/ghostsnap-*

# Debug
gdb ./target/debug/deps/ghostsnap-abc123
```

## CI Integration

Tests run automatically on:

- Pull requests
- Pushes to main
- Scheduled nightly builds

### GitHub Actions Example

```yaml
test:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - run: cargo test --all-features
```

## Performance Testing

### Benchmarks

```bash
# Run benchmarks
cargo bench

# Specific benchmark
cargo bench chunk
```

### Profiling

```bash
# Build with profiling
cargo build --release

# Profile with perf
perf record ./target/release/ghostsnap backup ...
perf report
```

## Test Fixtures

Located in `tests/fixtures/`:

```
tests/fixtures/
├── small_repo/         # Pre-initialized test repo
├── sample_files/       # Various file types
└── corrupted/          # Intentionally corrupted data
```

Use in tests:

```rust
let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("tests/fixtures/small_repo");
```
