//! Azure Blob Storage integration tests.
//!
//! These tests are opt-in and require Azure credentials to run.
//!
//! ## Required Environment Variables
//!
//! - `GHOSTSNAP_TEST_AZURE=1` - Enable Azure tests
//! - `GHOSTSNAP_TEST_AZURE_ACCOUNT` - Azure storage account name
//! - `GHOSTSNAP_TEST_AZURE_CONTAINER` - Azure container name
//! - `AZURE_STORAGE_KEY` - Azure storage account key
//!
//! ## Optional Environment Variables
//!
//! - `GHOSTSNAP_TEST_AZURE_PREFIX` - Blob prefix (default: unique per run)
//! - `GHOSTSNAP_TEST_AZURE_PASSWORD` - Repository password (default: "test-password")
//!
//! ## Example
//!
//! ```bash
//! export GHOSTSNAP_TEST_AZURE=1
//! export GHOSTSNAP_TEST_AZURE_ACCOUNT=mystorageaccount
//! export GHOSTSNAP_TEST_AZURE_CONTAINER=ghostsnap-tests
//! export AZURE_STORAGE_KEY=your-storage-key
//! cargo test azure --test azure_integration
//! ```

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ghostsnap_core::chunker::Chunker;
use ghostsnap_core::pack::PackManager;
use ghostsnap_core::snapshot::{Snapshot, Tree};
use ghostsnap_core::storage::{AzureLocation, RepositoryLocation};
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

fn azure_test_config() -> Option<(RepositoryLocation, String)> {
    if env::var("GHOSTSNAP_TEST_AZURE").ok().as_deref() != Some("1") {
        return None;
    }

    let account_name = env::var("GHOSTSNAP_TEST_AZURE_ACCOUNT").ok()?;
    let container = env::var("GHOSTSNAP_TEST_AZURE_CONTAINER").ok()?;
    let prefix = env::var("GHOSTSNAP_TEST_AZURE_PREFIX").unwrap_or_else(|_| {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        format!("ghostsnap-test-{}", nonce)
    });
    let password =
        env::var("GHOSTSNAP_TEST_AZURE_PASSWORD").unwrap_or_else(|_| "test-password".to_string());

    // Verify AZURE_STORAGE_KEY is set
    if env::var("AZURE_STORAGE_KEY").is_err() && env::var("AZURE_STORAGE_ACCESS_KEY").is_err() {
        eprintln!("Azure storage key not set (AZURE_STORAGE_KEY or AZURE_STORAGE_ACCESS_KEY)");
        return None;
    }

    let location = AzureLocation::new(account_name, container, prefix);
    Some((RepositoryLocation::Azure(location), password))
}

/// Tests basic Azure repository init, backup, and reopen.
#[tokio::test]
async fn test_azure_repository_roundtrip_opt_in() {
    let Some((location, password)) = azure_test_config() else {
        eprintln!("Skipping Azure integration test; set GHOSTSNAP_TEST_AZURE=1 and Azure env vars");
        return;
    };

    let source_dir = tempdir().unwrap();
    create_test_file(source_dir.path().join("hello.txt"), b"hello from azure");
    create_test_file(
        source_dir.path().join("nested/world.txt"),
        b"nested azure data",
    );

    // Initialize repository
    let repo = Repository::init_at_location(location.clone(), &password)
        .await
        .expect("Failed to init Azure repository");

    // Backup
    let snapshot_id = backup_dir(&repo, source_dir.path())
        .await
        .expect("Failed to backup to Azure");

    // Reopen and verify
    let reopened = Repository::open_at_location(location.clone(), &password)
        .await
        .expect("Failed to reopen Azure repository");

    // Verify transport config is persisted
    match reopened.config().transport.as_ref() {
        Some(RepoTransport::Azure(config)) => {
            if let RepositoryLocation::Azure(input) = &location {
                assert_eq!(config.account_name, input.account_name);
                assert_eq!(config.container, input.container);
                assert_eq!(config.prefix, input.prefix);
            } else {
                panic!("expected azure repository location");
            }
        }
        _ => panic!("expected persisted azure transport config"),
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

/// Tests copying snapshots from local to Azure.
#[tokio::test]
async fn test_copy_local_to_azure_opt_in() {
    let Some((azure_location, password)) = azure_test_config() else {
        eprintln!("Skipping Azure copy test; set GHOSTSNAP_TEST_AZURE=1 and Azure env vars");
        return;
    };

    let local_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    create_test_file(source_dir.path().join("copy.txt"), b"copy me to azure");

    // Create local repo and backup
    let local_repo = Repository::init(local_dir.path(), &password).await.unwrap();
    let snapshot_id = backup_dir(&local_repo, source_dir.path()).await.unwrap();

    // Create Azure repo and copy
    let azure_repo = Repository::init_at_location(azure_location.clone(), &password)
        .await
        .expect("Failed to init Azure repository for copy");
    copy_snapshot(&local_repo, &azure_repo, &snapshot_id)
        .await
        .expect("Failed to copy snapshot to Azure");

    // Verify snapshot exists in Azure
    let reopened = Repository::open_at_location(azure_location, &password)
        .await
        .unwrap();
    let snapshots = reopened.list_snapshots().await.unwrap();
    assert!(
        snapshots.iter().any(|id| id == &snapshot_id),
        "Snapshot should exist in Azure repository"
    );
}

/// Tests Azure repository listing with prefix.
#[tokio::test]
async fn test_azure_listing_with_prefix_opt_in() {
    let Some((location, password)) = azure_test_config() else {
        eprintln!("Skipping Azure listing test; set GHOSTSNAP_TEST_AZURE=1 and Azure env vars");
        return;
    };

    // Verify we have a prefix
    if let RepositoryLocation::Azure(ref az_loc) = location {
        assert!(
            !az_loc.prefix.is_empty(),
            "Test requires a non-empty prefix to validate listing"
        );
    }

    let source_dir = tempdir().unwrap();
    create_test_file(
        source_dir.path().join("list-test.txt"),
        b"azure listing test",
    );

    let repo = Repository::init_at_location(location.clone(), &password)
        .await
        .expect("Failed to init Azure repository");
    let _snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen and verify listing
    let reopened = Repository::open_at_location(location, &password)
        .await
        .unwrap();

    let snapshots = reopened.list_snapshots().await.unwrap();
    assert!(!snapshots.is_empty(), "Should find at least one snapshot");

    let packs = reopened.list_packs().await.unwrap();
    assert!(!packs.is_empty(), "Should find at least one pack");
}

/// Tests Azure chunk read/write roundtrip.
#[tokio::test]
async fn test_azure_chunk_roundtrip_opt_in() {
    let Some((location, password)) = azure_test_config() else {
        eprintln!("Skipping Azure chunk test; set GHOSTSNAP_TEST_AZURE=1 and Azure env vars");
        return;
    };

    let source_dir = tempdir().unwrap();
    // Create a file large enough to produce multiple chunks
    let large_data: Vec<u8> = (0..256 * 1024).map(|i| (i % 256) as u8).collect();
    create_test_file(source_dir.path().join("large.bin"), &large_data);

    let repo = Repository::init_at_location(location.clone(), &password)
        .await
        .expect("Failed to init Azure repository");
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen and read back chunks
    let reopened = Repository::open_at_location(location, &password)
        .await
        .unwrap();

    let snapshot = reopened.load_snapshot(&snapshot_id).await.unwrap();
    let tree = reopened.load_tree(&snapshot.tree).await.unwrap();

    // Find the large file and read its chunks
    let large_node = tree
        .nodes
        .iter()
        .find(|n| n.name == "large.bin")
        .expect("Should find large.bin");

    let mut reconstructed = Vec::new();
    for chunk_ref in &large_node.chunks {
        let chunk_data = reopened.load_chunk(&chunk_ref.id).await.unwrap();
        reconstructed.extend_from_slice(&chunk_data);
    }

    assert_eq!(
        reconstructed, large_data,
        "Reconstructed data should match original"
    );
}
