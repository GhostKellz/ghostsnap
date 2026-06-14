//! Integration tests for ghostsnap backup/restore cycle.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

use ghostsnap_core::chunker::Chunker;
use ghostsnap_core::pack::PackManager;
use ghostsnap_core::snapshot::{Snapshot, Tree};
use ghostsnap_core::storage::RepositoryLocation;
use ghostsnap_core::{ChunkRef, NodeType, RepoTransport, Repository, S3RepoSse, TreeNode};

/// Helper to create a test file with given contents.
fn create_test_file<P: AsRef<Path>>(path: P, contents: &[u8]) {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent).ok();
    }
    let mut file = File::create(path).unwrap();
    file.write_all(contents).unwrap();
}

/// Helper to assert two files have equal contents.
fn assert_files_equal<P1: AsRef<Path>, P2: AsRef<Path>>(path1: P1, path2: P2) {
    let content1 = fs::read(path1.as_ref()).unwrap();
    let content2 = fs::read(path2.as_ref()).unwrap();
    assert_eq!(content1, content2, "File contents differ");
}

/// Performs a backup of a source directory to a repository.
async fn backup_dir(repo: &Repository, source: &Path) -> anyhow::Result<String> {
    use walkdir::WalkDir;

    let chunker = Chunker::new_default();
    let mut pack_manager = PackManager::new(64 * 1024 * 1024);
    let mut tree = Tree::new();

    for entry in WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let relative = path.strip_prefix(source).unwrap_or(path);
        let metadata = entry.metadata()?;

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
            let data = fs::read(path)?;
            for chunk in chunker.chunk_data(&data) {
                let chunk_id = chunk.id();
                if !repo.has_chunk(&chunk_id).await?
                    && let Some(pack) = pack_manager.add_chunk(chunk_id, chunk.data())?
                {
                    repo.save_pack(&pack).await?;
                    for (cid, ce) in &pack.chunks {
                        repo.save_chunk_location(cid, &pack.header.pack_id, ce.offset, ce.length)
                            .await?;
                    }
                }
                chunks.push(ChunkRef {
                    id: chunk_id,
                    offset: 0,
                    length: chunk.data().len() as u32,
                });
            }
        }

        let link_target = if metadata.is_symlink() {
            fs::read_link(path)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        } else {
            None
        };

        tree.add_node(TreeNode {
            name: relative.to_string_lossy().to_string(),
            node_type,
            mode,
            uid,
            gid,
            size: metadata.len(),
            mtime,
            link_target,
            subtree_id: None,
            chunks,
            xattr: None,
            sparse_holes: None,
            inode: None,
            nlink: None,
            hardlink_target: None,
        });
    }

    // Save final pack
    if let Some(pack) = pack_manager.finish_current_pack() {
        repo.save_pack(&pack).await?;
        for (cid, ce) in &pack.chunks {
            repo.save_chunk_location(cid, &pack.header.pack_id, ce.offset, ce.length)
                .await?;
        }
    }

    let tree_id = repo.save_tree(&tree).await?;
    let snapshot = Snapshot::new(vec![source.to_path_buf()], tree_id);
    repo.save_snapshot(&snapshot).await?;
    repo.save_index().await?;

    Ok(snapshot.id)
}

/// Restores a snapshot to a target directory.
async fn restore_snapshot(
    repo: &Repository,
    snapshot_id: &str,
    target: &Path,
) -> anyhow::Result<()> {
    let snapshot = repo.load_snapshot(&snapshot_id.to_string()).await?;
    let tree = repo.load_tree(&snapshot.tree).await?;

    fs::create_dir_all(target)?;

    for node in &tree.nodes {
        let dest = target.join(&node.name);

        match node.node_type {
            NodeType::Directory => {
                fs::create_dir_all(&dest)?;
            }
            NodeType::File => {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }

                let mut data = Vec::new();
                for chunk_ref in &node.chunks {
                    let chunk_data = repo.load_chunk(&chunk_ref.id).await?;
                    data.extend_from_slice(&chunk_data);
                }
                fs::write(&dest, &data)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&dest, fs::Permissions::from_mode(node.mode))?;
                }
            }
            NodeType::Symlink => {
                if let Some(target_path) = &node.link_target {
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(target_path, &dest)?;
                }
            }
        }
    }

    Ok(())
}

/// Tests basic backup and restore of regular files.
#[tokio::test]
async fn test_basic_backup_restore() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let restore_dir = tempdir().unwrap();

    // Initialize repository
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create test files
    create_test_file(source_dir.path().join("hello.txt"), b"Hello, World!");
    create_test_file(source_dir.path().join("data.bin"), &[0u8; 1024]);
    fs::create_dir(source_dir.path().join("subdir")).unwrap();
    create_test_file(
        source_dir.path().join("subdir/nested.txt"),
        b"Nested file content",
    );

    // Backup
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Restore
    restore_snapshot(&repo, &snapshot_id, restore_dir.path())
        .await
        .unwrap();

    // Verify
    assert_files_equal(
        source_dir.path().join("hello.txt"),
        restore_dir.path().join("hello.txt"),
    );
    assert_files_equal(
        source_dir.path().join("data.bin"),
        restore_dir.path().join("data.bin"),
    );
    assert_files_equal(
        source_dir.path().join("subdir/nested.txt"),
        restore_dir.path().join("subdir/nested.txt"),
    );
}

/// Tests backup and restore of symlinks.
#[tokio::test]
#[cfg(unix)]
async fn test_symlink_backup_restore() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let restore_dir = tempdir().unwrap();

    // Initialize repository
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create file and symlink
    create_test_file(source_dir.path().join("target.txt"), b"Target content");
    std::os::unix::fs::symlink("target.txt", source_dir.path().join("link.txt")).unwrap();

    // Backup and restore
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();
    restore_snapshot(&repo, &snapshot_id, restore_dir.path())
        .await
        .unwrap();

    // Verify symlink
    let link_path = restore_dir.path().join("link.txt");
    assert!(
        link_path
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink(),
        "Should be a symlink"
    );
    assert_eq!(
        fs::read_link(&link_path).unwrap().to_string_lossy(),
        "target.txt"
    );
}

/// Tests backup and restore of file permissions.
#[tokio::test]
#[cfg(unix)]
async fn test_permissions_backup_restore() {
    use std::os::unix::fs::PermissionsExt;

    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let restore_dir = tempdir().unwrap();

    // Initialize repository
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create file with specific permissions
    let file_path = source_dir.path().join("executable.sh");
    create_test_file(&file_path, b"#!/bin/bash\necho hello");
    fs::set_permissions(&file_path, fs::Permissions::from_mode(0o755)).unwrap();

    // Backup and restore
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();
    restore_snapshot(&repo, &snapshot_id, restore_dir.path())
        .await
        .unwrap();

    // Verify permissions
    let restored_path = restore_dir.path().join("executable.sh");
    let mode = fs::metadata(&restored_path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755, "Permissions should be preserved");
}

/// Tests deduplication across backups.
#[tokio::test]
async fn test_deduplication() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize repository
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create test file
    let large_data = vec![0xABu8; 1024 * 1024]; // 1MB
    create_test_file(source_dir.path().join("large.bin"), &large_data);

    // First backup
    let snapshot1 = backup_dir(&repo, source_dir.path()).await.unwrap();
    let stats1 = repo.stats().await;

    // Second backup (same data)
    let snapshot2 = backup_dir(&repo, source_dir.path()).await.unwrap();
    let stats2 = repo.stats().await;

    // Chunk count should be the same (deduplication working)
    assert_ne!(snapshot1, snapshot2, "Snapshots should have different IDs");
    assert_eq!(
        stats1.chunk_count, stats2.chunk_count,
        "No new chunks should be created - deduplication should work"
    );
}

/// Tests large file backup/restore.
#[tokio::test]
async fn test_large_file() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let restore_dir = tempdir().unwrap();

    // Initialize repository
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create 5MB file with pattern data
    let large_data: Vec<u8> = (0..5 * 1024 * 1024).map(|i| (i % 256) as u8).collect();
    create_test_file(source_dir.path().join("large.bin"), &large_data);

    // Backup and restore
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();
    restore_snapshot(&repo, &snapshot_id, restore_dir.path())
        .await
        .unwrap();

    // Verify
    assert_files_equal(
        source_dir.path().join("large.bin"),
        restore_dir.path().join("large.bin"),
    );
}

/// Tests pack cache effectiveness.
#[tokio::test]
async fn test_pack_cache() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize repository
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create files that will fit in one pack
    for i in 0..10 {
        create_test_file(
            source_dir.path().join(format!("file{}.txt", i)),
            format!("Content of file {}", i).as_bytes(),
        );
    }

    // Backup
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen repository to clear any state
    drop(repo);
    let repo = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Load snapshot and tree to trigger pack reads
    let snapshot = repo.load_snapshot(&snapshot_id).await.unwrap();
    let tree = repo.load_tree(&snapshot.tree).await.unwrap();

    // Read all chunks - this should hit the cache after first read
    for node in &tree.nodes {
        for chunk_ref in &node.chunks {
            repo.load_chunk(&chunk_ref.id).await.unwrap();
        }
    }

    // Check cache stats
    let cache_stats = repo.cache_stats().await;
    assert!(cache_stats.pack_count > 0, "Cache should have packs loaded");
}

/// Tests multiple snapshots with different content.
#[tokio::test]
async fn test_multiple_snapshots() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let restore_dir1 = tempdir().unwrap();
    let restore_dir2 = tempdir().unwrap();

    // Initialize repository
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // First backup
    create_test_file(source_dir.path().join("file.txt"), b"Version 1");
    let snapshot1 = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Modify and second backup
    create_test_file(source_dir.path().join("file.txt"), b"Version 2");
    create_test_file(source_dir.path().join("new.txt"), b"New file");
    let snapshot2 = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Restore both snapshots
    restore_snapshot(&repo, &snapshot1, restore_dir1.path())
        .await
        .unwrap();
    restore_snapshot(&repo, &snapshot2, restore_dir2.path())
        .await
        .unwrap();

    // Verify first snapshot
    assert_eq!(
        fs::read_to_string(restore_dir1.path().join("file.txt")).unwrap(),
        "Version 1"
    );
    assert!(!restore_dir1.path().join("new.txt").exists());

    // Verify second snapshot
    assert_eq!(
        fs::read_to_string(restore_dir2.path().join("file.txt")).unwrap(),
        "Version 2"
    );
    assert_eq!(
        fs::read_to_string(restore_dir2.path().join("new.txt")).unwrap(),
        "New file"
    );
}

/// Tests empty directory handling.
#[tokio::test]
async fn test_empty_directory() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let restore_dir = tempdir().unwrap();

    // Initialize repository
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create empty subdirectory
    fs::create_dir_all(source_dir.path().join("empty_dir")).unwrap();
    create_test_file(source_dir.path().join("file.txt"), b"Content");

    // Backup and restore
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();
    restore_snapshot(&repo, &snapshot_id, restore_dir.path())
        .await
        .unwrap();

    // Verify empty directory exists
    assert!(
        restore_dir.path().join("empty_dir").is_dir(),
        "Empty directory should be restored"
    );
}

#[test]
fn test_repository_location_parse_local() {
    let location = RepositoryLocation::parse("/backup/repo").unwrap();
    match location {
        RepositoryLocation::Local(path) => {
            assert_eq!(path, std::path::PathBuf::from("/backup/repo"))
        }
        RepositoryLocation::S3(_) => panic!("expected local repository location"),
        RepositoryLocation::Azure(_) => panic!("expected local repository location"),
        RepositoryLocation::Rclone(_) => panic!("expected local repository location"),
        RepositoryLocation::Sftp(_) => panic!("expected local repository location"),
    }
}

#[test]
fn test_repository_location_parse_s3() {
    let location = RepositoryLocation::parse("s3:my-bucket/ghostsnap/main").unwrap();
    match location {
        RepositoryLocation::S3(s3) => {
            assert_eq!(s3.bucket, "my-bucket");
            assert_eq!(s3.prefix, "ghostsnap/main");
        }
        RepositoryLocation::Local(_) => panic!("expected s3 repository location"),
        RepositoryLocation::Azure(_) => panic!("expected s3 repository location"),
        RepositoryLocation::Rclone(_) => panic!("expected s3 repository location"),
        RepositoryLocation::Sftp(_) => panic!("expected s3 repository location"),
    }
}

#[test]
fn test_repository_location_parse_sftp() {
    let location = RepositoryLocation::parse("sftp:backup@nas.local:2222/srv/ghostsnap").unwrap();
    match location {
        RepositoryLocation::Sftp(sftp) => {
            assert_eq!(sftp.user, "backup");
            assert_eq!(sftp.host, "nas.local");
            assert_eq!(sftp.port, 2222);
            assert_eq!(sftp.path, "srv/ghostsnap");
        }
        _ => panic!("expected sftp repository location"),
    }
}

#[test]
fn test_repository_location_parse_sftp_defaults() {
    // No user, no port, no path: defaults to port 22 and empty user/path.
    let location = RepositoryLocation::parse("sftp://example.com").unwrap();
    match location {
        RepositoryLocation::Sftp(sftp) => {
            assert_eq!(sftp.host, "example.com");
            assert_eq!(sftp.port, 22);
            assert!(sftp.user.is_empty());
            assert!(sftp.path.is_empty());
        }
        _ => panic!("expected sftp repository location"),
    }
}

#[tokio::test]
async fn test_open_init_at_location_local() {
    let repo_dir = tempdir().unwrap();
    let location = RepositoryLocation::Local(repo_dir.path().join("repo"));

    let repo = Repository::init_at_location(location.clone(), "test-password")
        .await
        .unwrap();
    assert_eq!(repo.location().display(), location.display());

    let reopened = Repository::open_at_location(location.clone(), "test-password")
        .await
        .unwrap();
    assert_eq!(reopened.location().display(), location.display());
}

#[tokio::test]
async fn test_s3_transport_config_persists_in_repo_config() {
    let repo_dir = tempdir().unwrap();
    let mut repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    let mut location = ghostsnap_core::storage::S3Location::new(
        "example-bucket".to_string(),
        "ghostsnap/backups".to_string(),
    );
    location.endpoint = Some("https://s3.example.com".to_string());
    location.region = Some("us-east-2".to_string());

    repo.set_s3_transport_config(
        &location,
        Some(S3RepoSse {
            mode: "kms".to_string(),
            kms_key_id: Some("alias/ghostsnap".to_string()),
        }),
    )
    .await
    .unwrap();

    let reopened = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    match reopened.config().transport.as_ref() {
        Some(RepoTransport::S3(config)) => {
            assert_eq!(config.bucket, "example-bucket");
            assert_eq!(config.prefix, "ghostsnap/backups");
            assert_eq!(config.endpoint.as_deref(), Some("https://s3.example.com"));
            assert_eq!(config.region.as_deref(), Some("us-east-2"));
            assert_eq!(config.sse.as_ref().map(|s| s.mode.as_str()), Some("kms"));
            assert_eq!(
                config.sse.as_ref().and_then(|s| s.kms_key_id.as_deref()),
                Some("alias/ghostsnap")
            );
        }
        _ => panic!("expected persisted S3 transport config"),
    }
}

#[test]
fn test_azure_repository_location_parse() {
    // Azure is now supported
    let azure = RepositoryLocation::parse("azure:myaccount/mycontainer/prefix").unwrap();
    match azure {
        RepositoryLocation::Azure(az) => {
            assert_eq!(az.account_name, "myaccount");
            assert_eq!(az.container, "mycontainer");
            assert_eq!(az.prefix, "prefix");
        }
        _ => panic!("expected azure repository location"),
    }
}

#[test]
fn test_b2_minio_parse_as_s3() {
    // Backblaze B2 and MinIO are S3-compatible and map to an S3 location.
    let b2 = RepositoryLocation::parse("b2:bucket/repo").unwrap();
    match b2 {
        RepositoryLocation::S3(s3) => {
            assert_eq!(s3.bucket, "bucket");
            assert_eq!(s3.prefix, "repo");
        }
        _ => panic!("expected b2 to map to an s3 repository location"),
    }

    let minio = RepositoryLocation::parse("minio://data/ghostsnap").unwrap();
    match minio {
        RepositoryLocation::S3(s3) => {
            assert_eq!(s3.bucket, "data");
            assert_eq!(s3.prefix, "ghostsnap");
        }
        _ => panic!("expected minio to map to an s3 repository location"),
    }
}

#[test]
fn test_rclone_repository_location_parse() {
    let location = RepositoryLocation::parse("rclone:myremote/backups/ghostsnap").unwrap();
    match location {
        RepositoryLocation::Rclone(rclone) => {
            assert_eq!(rclone.remote, "myremote");
            assert_eq!(rclone.path, "backups/ghostsnap");
        }
        _ => panic!("expected rclone repository location"),
    }

    // Test with just remote, no path
    let location2 = RepositoryLocation::parse("rclone:gdrive").unwrap();
    match location2 {
        RepositoryLocation::Rclone(rclone) => {
            assert_eq!(rclone.remote, "gdrive");
            assert_eq!(rclone.path, "");
        }
        _ => panic!("expected rclone repository location"),
    }
}

#[test]
fn test_s3_location_env_overrides() {
    use ghostsnap_core::storage::S3Location;

    // SAFETY: This test modifies env vars but runs in a single thread.
    // Using serial_test or similar would be needed for parallel safety.
    unsafe {
        // Set test env vars
        std::env::set_var("AWS_ENDPOINT_URL", "https://test.example.com");
        std::env::set_var("AWS_REGION", "us-west-2");
    }

    let location = S3Location::new("test-bucket".to_string(), "prefix".to_string());
    assert!(location.endpoint.is_none());
    assert!(location.region.is_none());

    let location = location.with_env_overrides();
    assert_eq!(
        location.endpoint,
        Some("https://test.example.com".to_string())
    );
    assert_eq!(location.region, Some("us-west-2".to_string()));

    // SAFETY: Cleaning up env vars set by this test
    unsafe {
        std::env::remove_var("AWS_ENDPOINT_URL");
        std::env::remove_var("AWS_REGION");
    }

    // Test that explicit values are not overwritten
    // SAFETY: Setting temp env var for this test
    unsafe {
        std::env::set_var("AWS_ENDPOINT_URL", "https://should-not-use.com");
    }

    let location = S3Location {
        bucket: "bucket".to_string(),
        prefix: "prefix".to_string(),
        endpoint: Some("https://explicit.example.com".to_string()),
        region: None,
        sse: None,
    };

    let location = location.with_env_overrides();
    assert_eq!(
        location.endpoint,
        Some("https://explicit.example.com".to_string())
    );

    // SAFETY: Cleanup
    unsafe {
        std::env::remove_var("AWS_ENDPOINT_URL");
    }
}

#[tokio::test]
async fn test_s3_sse_config_persists_and_resolves() {
    use ghostsnap_core::storage::S3Location;

    let repo_dir = tempdir().unwrap();
    let mut repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    let mut location = S3Location::new("sse-test-bucket".to_string(), "backups".to_string());
    location.endpoint = Some("https://s3.example.com".to_string());

    // Set SSE config with AES256
    repo.set_s3_transport_config(
        &location,
        Some(S3RepoSse {
            mode: "aes256".to_string(),
            kms_key_id: None,
        }),
    )
    .await
    .unwrap();

    // Reopen and verify SSE is persisted
    let reopened = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    match reopened.config().transport.as_ref() {
        Some(RepoTransport::S3(config)) => {
            assert_eq!(config.sse.as_ref().map(|s| s.mode.as_str()), Some("aes256"));
            assert!(
                config
                    .sse
                    .as_ref()
                    .and_then(|s| s.kms_key_id.as_ref())
                    .is_none()
            );
        }
        _ => panic!("expected persisted S3 transport config with SSE"),
    }
}
