//! Command-level smoke tests for Ghostsnap CLI.
//!
//! These tests verify that maintenance commands work correctly
//! through end-to-end local repository workflows.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

use ghostsnap_core::chunker::Chunker;
use ghostsnap_core::pack::PackManager;
use ghostsnap_core::snapshot::{Snapshot, Tree};
use ghostsnap_core::{ChunkRef, NodeType, Repository, TreeNode};

/// Helper to create a test file with given contents.
fn create_test_file<P: AsRef<Path>>(path: P, contents: &[u8]) {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent).ok();
    }
    let mut file = File::create(path).unwrap();
    file.write_all(contents).unwrap();
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

/// Tests repository verify command functionality.
#[tokio::test]
async fn test_verify_command() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize and backup
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();
    create_test_file(source_dir.path().join("data.txt"), b"verify test data");
    let _snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen repository (simulates separate command invocation)
    let repo = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Verify check passes
    let stats = repo.verify(false).await.unwrap();

    assert!(stats.valid_snapshots > 0, "Should verify at least one snapshot");
    assert!(stats.valid_packs > 0, "Should verify at least one pack");
    assert_eq!(stats.corrupt_packs, 0, "Should have no corrupt packs");
    assert_eq!(stats.corrupt_snapshots, 0, "Should have no corrupt snapshots");
}

/// Tests repository verify with data validation.
#[tokio::test]
async fn test_verify_with_data_check() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize and backup
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();
    create_test_file(source_dir.path().join("data.txt"), b"verify with data check");
    let _snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen and verify with data check
    let repo = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    let stats = repo.verify(true).await.unwrap();

    assert!(stats.valid_packs > 0, "Should have valid packs");
    assert_eq!(stats.corrupt_packs, 0, "Should have no corrupt packs");
}

/// Tests repository stats command functionality.
#[tokio::test]
async fn test_stats_command() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize and backup
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();
    create_test_file(source_dir.path().join("stats.txt"), b"stats test data");
    let _snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen and get stats
    let repo = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    let stats = repo.stats().await;
    assert!(stats.chunk_count > 0, "Should have at least one chunk");
    assert!(stats.pack_count > 0, "Should have at least one pack");
}

/// Tests ls command functionality (listing files in snapshot).
#[tokio::test]
async fn test_ls_command() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize and backup
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();
    create_test_file(source_dir.path().join("file1.txt"), b"file 1");
    create_test_file(source_dir.path().join("dir/file2.txt"), b"file 2");
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen and list files
    let repo = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    let snapshot = repo.load_snapshot(&snapshot_id).await.unwrap();
    let tree = repo.load_tree(&snapshot.tree).await.unwrap();

    let file_names: Vec<&str> = tree.nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(file_names.contains(&"file1.txt"));
    assert!(file_names.iter().any(|n| *n == "dir/file2.txt" || *n == "dir"));
}

/// Tests diff command functionality (comparing snapshots).
#[tokio::test]
async fn test_diff_command() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // First backup
    create_test_file(source_dir.path().join("unchanged.txt"), b"same content");
    create_test_file(source_dir.path().join("modified.txt"), b"version 1");
    let snapshot1 = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Modify and second backup
    create_test_file(source_dir.path().join("modified.txt"), b"version 2");
    create_test_file(source_dir.path().join("added.txt"), b"new file");
    let snapshot2 = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen and compare
    let repo = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    let tree1 = repo
        .load_tree(&repo.load_snapshot(&snapshot1).await.unwrap().tree)
        .await
        .unwrap();
    let tree2 = repo
        .load_tree(&repo.load_snapshot(&snapshot2).await.unwrap().tree)
        .await
        .unwrap();

    // Check that trees are different
    assert_ne!(
        tree1.nodes.len(),
        tree2.nodes.len(),
        "Trees should have different node counts"
    );

    // Check that added.txt exists only in tree2
    assert!(
        !tree1.nodes.iter().any(|n| n.name == "added.txt"),
        "added.txt should not exist in snapshot 1"
    );
    assert!(
        tree2.nodes.iter().any(|n| n.name == "added.txt"),
        "added.txt should exist in snapshot 2"
    );
}

/// Tests dump command functionality (extracting single file).
#[tokio::test]
async fn test_dump_command() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    let test_content = b"content to dump from snapshot";

    // Initialize and backup
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();
    create_test_file(source_dir.path().join("dump.txt"), test_content);
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen and dump file
    let repo = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    let snapshot = repo.load_snapshot(&snapshot_id).await.unwrap();
    let tree = repo.load_tree(&snapshot.tree).await.unwrap();

    // Find the file and read its content
    let node = tree
        .nodes
        .iter()
        .find(|n| n.name == "dump.txt")
        .expect("Should find dump.txt");

    let mut content = Vec::new();
    for chunk_ref in &node.chunks {
        let chunk_data = repo.load_chunk(&chunk_ref.id).await.unwrap();
        content.extend_from_slice(&chunk_data);
    }

    assert_eq!(content, test_content, "Dumped content should match original");
}

/// Tests forget command functionality (snapshot deletion).
#[tokio::test]
async fn test_forget_command() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create multiple snapshots
    create_test_file(source_dir.path().join("data.txt"), b"v1");
    let snapshot1 = backup_dir(&repo, source_dir.path()).await.unwrap();

    create_test_file(source_dir.path().join("data.txt"), b"v2");
    let snapshot2 = backup_dir(&repo, source_dir.path()).await.unwrap();

    create_test_file(source_dir.path().join("data.txt"), b"v3");
    let snapshot3 = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Verify we have 3 snapshots
    let snapshots = repo.list_snapshots().await.unwrap();
    assert_eq!(snapshots.len(), 3, "Should have 3 snapshots");

    // Delete first snapshot (simulates forget)
    repo.delete_snapshot(&snapshot1).await.unwrap();

    // Verify only 2 remain
    let snapshots = repo.list_snapshots().await.unwrap();
    assert_eq!(snapshots.len(), 2, "Should have 2 snapshots after forget");
    assert!(!snapshots.contains(&snapshot1), "Snapshot 1 should be forgotten");
    assert!(snapshots.contains(&snapshot2), "Snapshot 2 should remain");
    assert!(snapshots.contains(&snapshot3), "Snapshot 3 should remain");
}

/// Tests prune command functionality (removing unreferenced data).
#[tokio::test]
async fn test_prune_command() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create unique data for first snapshot
    let unique_data: Vec<u8> = (0..64 * 1024).map(|i| (i % 256) as u8).collect();
    create_test_file(source_dir.path().join("unique.bin"), &unique_data);
    let snapshot1 = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Get pack count before prune
    let packs_before = repo.list_packs().await.unwrap().len();
    assert!(packs_before > 0, "Should have packs before prune");

    // Delete the snapshot (data becomes unreferenced)
    repo.delete_snapshot(&snapshot1).await.unwrap();

    // Verify snapshot is gone
    let snapshots = repo.list_snapshots().await.unwrap();
    assert!(snapshots.is_empty(), "Should have no snapshots");

    // Prune unreferenced data
    let prune_stats = repo.prune_packs().await.unwrap();

    // Verify prune removed data
    assert!(
        prune_stats.packs_removed > 0 || prune_stats.chunks_removed > 0,
        "Prune should remove unreferenced data"
    );

    // Verify repository is still valid
    let verify_result = repo.verify(false).await;
    assert!(verify_result.is_ok(), "Repository should still be valid after prune");
}

/// Tests copy command functionality (copying between repositories).
#[tokio::test]
async fn test_copy_command() {
    let src_repo_dir = tempdir().unwrap();
    let dst_repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize source repo and backup
    let src_repo = Repository::init(src_repo_dir.path(), "test-password")
        .await
        .unwrap();
    create_test_file(source_dir.path().join("copy.txt"), b"copy this data");
    let snapshot_id = backup_dir(&src_repo, source_dir.path()).await.unwrap();

    // Initialize destination repo
    let dst_repo = Repository::init(dst_repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Copy snapshot
    let snapshot = src_repo.load_snapshot(&snapshot_id).await.unwrap();
    let tree = src_repo.load_tree(&snapshot.tree).await.unwrap();

    // Copy all chunks
    let mut chunks_needed = std::collections::HashSet::new();
    for node in &tree.nodes {
        for chunk_ref in &node.chunks {
            chunks_needed.insert(chunk_ref.id);
        }
    }

    let mut pack_manager = PackManager::new(64 * 1024 * 1024);
    for chunk_id in &chunks_needed {
        if !dst_repo.has_chunk(chunk_id).await.unwrap()
            && let Some(pack) =
                pack_manager.add_chunk(*chunk_id, &src_repo.load_chunk(chunk_id).await.unwrap()).unwrap()
        {
            dst_repo.save_pack(&pack).await.unwrap();
            for (cid, ce) in &pack.chunks {
                dst_repo
                    .save_chunk_location(cid, &pack.header.pack_id, ce.offset, ce.length)
                    .await
                    .unwrap();
            }
        }
    }

    if let Some(pack) = pack_manager.finish_current_pack() {
        dst_repo.save_pack(&pack).await.unwrap();
        for (cid, ce) in &pack.chunks {
            dst_repo
                .save_chunk_location(cid, &pack.header.pack_id, ce.offset, ce.length)
                .await
                .unwrap();
        }
    }

    // Copy tree and snapshot
    let dst_tree_id = dst_repo.save_tree(&tree).await.unwrap();
    let mut dst_snapshot = snapshot.clone();
    dst_snapshot.tree = dst_tree_id;
    dst_repo.save_snapshot(&dst_snapshot).await.unwrap();
    dst_repo.save_index().await.unwrap();

    // Verify copy
    let dst_snapshots = dst_repo.list_snapshots().await.unwrap();
    assert!(
        dst_snapshots.contains(&snapshot_id),
        "Copied snapshot should exist in destination"
    );

    // Verify content
    let dst_tree = dst_repo.load_tree(&dst_tree_id).await.unwrap();
    let node = dst_tree
        .nodes
        .iter()
        .find(|n| n.name == "copy.txt")
        .expect("Should find copy.txt");

    let mut content = Vec::new();
    for chunk_ref in &node.chunks {
        let chunk_data = dst_repo.load_chunk(&chunk_ref.id).await.unwrap();
        content.extend_from_slice(&chunk_data);
    }
    assert_eq!(content, b"copy this data", "Content should match");
}

/// Tests full maintenance workflow: backup -> verify -> forget -> prune -> verify.
#[tokio::test]
async fn test_maintenance_workflow() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    // Initialize
    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create multiple snapshots with varying data
    let mut snapshot_ids = Vec::new();
    for i in 0..5 {
        let data = format!("snapshot {} data with unique content {}", i, i * 1000);
        create_test_file(source_dir.path().join("data.txt"), data.as_bytes());
        let snapshot = backup_dir(&repo, source_dir.path()).await.unwrap();
        snapshot_ids.push(snapshot);
    }

    // Verify all snapshots exist
    let snapshots = repo.list_snapshots().await.unwrap();
    assert_eq!(snapshots.len(), 5, "Should have 5 snapshots");

    // Verify integrity
    let verify1 = repo.verify(false).await.unwrap();
    assert_eq!(verify1.corrupt_packs, 0, "Initial verify should have no errors");
    assert_eq!(verify1.corrupt_snapshots, 0, "Initial verify should have no corrupt snapshots");

    // Delete old snapshots (keep last 2)
    for snapshot_id in snapshot_ids.iter().take(3) {
        repo.delete_snapshot(snapshot_id).await.unwrap();
    }

    let remaining = repo.list_snapshots().await.unwrap();
    assert_eq!(remaining.len(), 2, "Should have 2 snapshots after forget");

    // Prune unreferenced data
    let prune_stats = repo.prune_packs().await.unwrap();
    assert!(
        prune_stats.chunks_removed > 0 || prune_stats.packs_removed > 0,
        "Should remove some data"
    );

    // Final integrity verify
    let verify2 = repo.verify(false).await.unwrap();
    assert_eq!(verify2.corrupt_packs, 0, "Final verify should have no errors");
    assert_eq!(verify2.corrupt_snapshots, 0, "Final verify should have no corrupt snapshots");

    // Verify remaining snapshots are still accessible
    for snapshot_id in &remaining {
        let snapshot = repo.load_snapshot(snapshot_id).await;
        assert!(snapshot.is_ok(), "Remaining snapshot should be accessible");
    }
}

/// Tests list snapshots functionality.
#[tokio::test]
async fn test_list_snapshots() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    // Create test files and backup
    create_test_file(source_dir.path().join("file.txt"), b"test");
    let snapshot1 = backup_dir(&repo, source_dir.path()).await.unwrap();

    create_test_file(source_dir.path().join("file2.txt"), b"test2");
    let snapshot2 = backup_dir(&repo, source_dir.path()).await.unwrap();

    // List snapshots
    let snapshots = repo.list_snapshots().await.unwrap();
    assert_eq!(snapshots.len(), 2, "Should have 2 snapshots");
    assert!(snapshots.contains(&snapshot1));
    assert!(snapshots.contains(&snapshot2));
}

/// Tests cache functionality.
#[tokio::test]
async fn test_cache_stats() {
    let repo_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();

    let repo = Repository::init(repo_dir.path(), "test-password")
        .await
        .unwrap();

    create_test_file(source_dir.path().join("cache.txt"), b"cache test data");
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen and load data to populate cache
    let repo = Repository::open(repo_dir.path(), "test-password")
        .await
        .unwrap();

    let snapshot = repo.load_snapshot(&snapshot_id).await.unwrap();
    let tree = repo.load_tree(&snapshot.tree).await.unwrap();

    // Load chunks to populate cache
    for node in &tree.nodes {
        for chunk_ref in &node.chunks {
            let _ = repo.load_chunk(&chunk_ref.id).await;
        }
    }

    let cache_stats = repo.cache_stats().await;
    assert!(cache_stats.pack_count > 0, "Cache should have packs loaded");
}
