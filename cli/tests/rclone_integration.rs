//! Rclone integration tests.
//!
//! These tests are opt-in and require rclone to be installed and configured.
//!
//! ## Required Environment Variables
//!
//! - `GHOSTSNAP_TEST_RCLONE=1` - Enable rclone tests
//! - `GHOSTSNAP_TEST_RCLONE_REMOTE` - Rclone remote name (e.g., "myremote")
//!
//! ## Optional Environment Variables
//!
//! - `GHOSTSNAP_TEST_RCLONE_PATH` - Path within remote (default: unique per run)
//! - `GHOSTSNAP_TEST_RCLONE_PASSWORD` - Repository password (default: "test-password")
//!
//! ## Prerequisites
//!
//! 1. Install rclone: https://rclone.org/install/
//! 2. Configure a remote: `rclone config`
//! 3. Verify it works: `rclone lsd myremote:`
//!
//! ## Example
//!
//! ```bash
//! export GHOSTSNAP_TEST_RCLONE=1
//! export GHOSTSNAP_TEST_RCLONE_REMOTE=myremote
//! cargo test rclone --test rclone_integration
//! ```

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ghostsnap_core::chunker::Chunker;
use ghostsnap_core::pack::PackManager;
use ghostsnap_core::snapshot::{Snapshot, Tree};
use ghostsnap_core::storage::{RcloneLocation, RepositoryLocation};
use ghostsnap_core::{ChunkRef, NodeType, RepoTransport, Repository, TreeNode};
use tempfile::tempdir;

fn create_test_file<P: AsRef<Path>>(path: P, contents: &[u8]) {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut file = fs::File::create(path).unwrap();
    file.write_all(contents).unwrap();
}

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

/// Copy a snapshot from one repository to another.
async fn copy_snapshot(
    src_repo: &Repository,
    dst_repo: &Repository,
    snapshot_id: &str,
) -> anyhow::Result<()> {
    let snapshot = src_repo.load_snapshot(&snapshot_id.to_string()).await?;
    let tree = src_repo.load_tree(&snapshot.tree).await?;

    let mut chunks_needed = std::collections::HashSet::new();
    for node in &tree.nodes {
        for chunk_ref in &node.chunks {
            chunks_needed.insert(chunk_ref.id);
        }
    }

    let mut pack_manager = PackManager::new(64 * 1024 * 1024);
    for chunk_id in &chunks_needed {
        if !dst_repo.has_chunk(chunk_id).await?
            && let Some(finished_pack) =
                pack_manager.add_chunk(*chunk_id, &src_repo.load_chunk(chunk_id).await?)?
        {
            dst_repo.save_pack(&finished_pack).await?;
            for (cid, entry) in &finished_pack.chunks {
                dst_repo
                    .save_chunk_location(
                        cid,
                        &finished_pack.header.pack_id,
                        entry.offset,
                        entry.length,
                    )
                    .await?;
            }
        }
    }

    if let Some(finished_pack) = pack_manager.finish_current_pack() {
        dst_repo.save_pack(&finished_pack).await?;
        for (cid, entry) in &finished_pack.chunks {
            dst_repo
                .save_chunk_location(
                    cid,
                    &finished_pack.header.pack_id,
                    entry.offset,
                    entry.length,
                )
                .await?;
        }
    }

    let dst_tree_id = dst_repo.save_tree(&tree).await?;
    let mut dst_snapshot = snapshot.clone();
    dst_snapshot.tree = dst_tree_id;
    dst_repo.save_snapshot(&dst_snapshot).await?;
    dst_repo.save_index().await?;
    Ok(())
}

fn rclone_test_config() -> Option<(RepositoryLocation, String)> {
    if env::var("GHOSTSNAP_TEST_RCLONE").ok().as_deref() != Some("1") {
        return None;
    }

    let remote = env::var("GHOSTSNAP_TEST_RCLONE_REMOTE").ok()?;
    let path = env::var("GHOSTSNAP_TEST_RCLONE_PATH").unwrap_or_else(|_| {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        format!("ghostsnap-test-{}", nonce)
    });
    let password =
        env::var("GHOSTSNAP_TEST_RCLONE_PASSWORD").unwrap_or_else(|_| "test-password".to_string());

    let location = RcloneLocation::new(remote, path);
    Some((RepositoryLocation::Rclone(location), password))
}

/// Tests basic rclone repository init, backup, and reopen.
#[tokio::test]
async fn test_rclone_repository_roundtrip_opt_in() {
    let Some((location, password)) = rclone_test_config() else {
        eprintln!("Skipping rclone integration test; set GHOSTSNAP_TEST_RCLONE=1 and GHOSTSNAP_TEST_RCLONE_REMOTE");
        return;
    };

    let source_dir = tempdir().unwrap();
    create_test_file(source_dir.path().join("hello.txt"), b"hello from rclone");
    create_test_file(
        source_dir.path().join("nested/world.txt"),
        b"nested rclone data",
    );

    // Initialize repository
    let repo = Repository::init_at_location(location.clone(), &password)
        .await
        .expect("Failed to init rclone repository");

    // Backup
    let snapshot_id = backup_dir(&repo, source_dir.path())
        .await
        .expect("Failed to backup to rclone");

    // Reopen and verify
    let reopened = Repository::open_at_location(location.clone(), &password)
        .await
        .expect("Failed to reopen rclone repository");

    // Verify transport config is persisted
    match reopened.config().transport.as_ref() {
        Some(RepoTransport::Rclone(config)) => {
            if let RepositoryLocation::Rclone(input) = &location {
                assert_eq!(config.remote, input.remote);
                assert_eq!(config.path, input.path);
            } else {
                panic!("expected rclone repository location");
            }
        }
        _ => panic!("expected persisted rclone transport config"),
    }

    // Verify snapshot and tree
    let snapshot = reopened.load_snapshot(&snapshot_id).await.unwrap();
    let tree = reopened.load_tree(&snapshot.tree).await.unwrap();

    assert!(tree.nodes.iter().any(|node| node.name == "hello.txt"));
    assert!(
        tree.nodes
            .iter()
            .any(|node| node.name == "nested/world.txt")
    );
    assert!(!reopened.list_packs().await.unwrap().is_empty());
}

/// Tests copying snapshots from local to rclone.
#[tokio::test]
async fn test_copy_local_to_rclone_opt_in() {
    let Some((rclone_location, password)) = rclone_test_config() else {
        eprintln!("Skipping rclone copy test; set GHOSTSNAP_TEST_RCLONE=1 and GHOSTSNAP_TEST_RCLONE_REMOTE");
        return;
    };

    // Use unique path for this test
    let rclone_location = if let RepositoryLocation::Rclone(loc) = rclone_location {
        RepositoryLocation::Rclone(RcloneLocation::new(loc.remote, format!("{}-copy", loc.path)))
    } else {
        panic!("expected rclone location");
    };

    let local_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    create_test_file(source_dir.path().join("copy.txt"), b"copy me to rclone");

    // Create local repo and backup
    let local_repo = Repository::init(local_dir.path(), &password).await.unwrap();
    let snapshot_id = backup_dir(&local_repo, source_dir.path()).await.unwrap();

    // Create rclone repo and copy
    let rclone_repo = Repository::init_at_location(rclone_location.clone(), &password)
        .await
        .expect("Failed to init rclone repository for copy");
    copy_snapshot(&local_repo, &rclone_repo, &snapshot_id)
        .await
        .expect("Failed to copy snapshot to rclone");

    // Verify snapshot exists in rclone
    let reopened = Repository::open_at_location(rclone_location, &password)
        .await
        .unwrap();
    let snapshots = reopened.list_snapshots().await.unwrap();
    assert!(
        snapshots.iter().any(|id| id == &snapshot_id),
        "Snapshot should exist in rclone repository"
    );
}

/// Tests rclone repository listing with nested path.
#[tokio::test]
async fn test_rclone_listing_with_prefix_opt_in() {
    let Some((location, password)) = rclone_test_config() else {
        eprintln!("Skipping rclone listing test; set GHOSTSNAP_TEST_RCLONE=1 and GHOSTSNAP_TEST_RCLONE_REMOTE");
        return;
    };

    // Use nested path for this test
    let location = if let RepositoryLocation::Rclone(loc) = location {
        RepositoryLocation::Rclone(RcloneLocation::new(loc.remote, format!("{}/nested/path", loc.path)))
    } else {
        panic!("expected rclone location");
    };

    // Initialize repository
    let repo = Repository::init_at_location(location.clone(), &password)
        .await
        .expect("Failed to init rclone repository with nested path");

    // List packs (should be empty but not error)
    let packs = repo.list_packs().await.expect("Failed to list packs");
    assert!(packs.is_empty(), "New repo should have no packs");

    // List snapshots (should be empty but not error)
    let snapshots = repo.list_snapshots().await.expect("Failed to list snapshots");
    assert!(snapshots.is_empty(), "New repo should have no snapshots");
}

/// Tests large file backup via rclone.
#[tokio::test]
async fn test_rclone_large_file_opt_in() {
    let Some((location, password)) = rclone_test_config() else {
        eprintln!("Skipping rclone large file test; set GHOSTSNAP_TEST_RCLONE=1 and GHOSTSNAP_TEST_RCLONE_REMOTE");
        return;
    };

    // Use unique path for this test
    let location = if let RepositoryLocation::Rclone(loc) = location {
        RepositoryLocation::Rclone(RcloneLocation::new(loc.remote, format!("{}-large", loc.path)))
    } else {
        panic!("expected rclone location");
    };

    let source_dir = tempdir().unwrap();
    // Create a ~1MB file to test chunking
    let large_data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
    create_test_file(source_dir.path().join("large.bin"), &large_data);

    // Initialize and backup
    let repo = Repository::init_at_location(location.clone(), &password)
        .await
        .expect("Failed to init rclone repository");

    let snapshot_id = backup_dir(&repo, source_dir.path())
        .await
        .expect("Failed to backup large file to rclone");

    // Verify
    let snapshot = repo.load_snapshot(&snapshot_id).await.unwrap();
    let tree = repo.load_tree(&snapshot.tree).await.unwrap();

    let large_node = tree.nodes.iter().find(|n| n.name == "large.bin").unwrap();
    assert_eq!(large_node.size, 1024 * 1024, "File size should match");
    assert!(!large_node.chunks.is_empty(), "Should have chunks");
}
