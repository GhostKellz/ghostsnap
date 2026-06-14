use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ghostsnap_core::chunker::Chunker;
use ghostsnap_core::pack::PackManager;
use ghostsnap_core::snapshot::{Snapshot, Tree};
use ghostsnap_core::storage::{RepositoryLocation, S3Location};
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

fn s3_test_config() -> Option<(RepositoryLocation, String)> {
    if env::var("GHOSTSNAP_TEST_S3").ok().as_deref() != Some("1") {
        return None;
    }

    let bucket = env::var("GHOSTSNAP_TEST_S3_BUCKET").ok()?;
    let prefix = env::var("GHOSTSNAP_TEST_S3_PREFIX").unwrap_or_else(|_| {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        format!("ghostsnap-test-{}", nonce)
    });
    let password =
        env::var("GHOSTSNAP_TEST_S3_PASSWORD").unwrap_or_else(|_| "test-password".to_string());

    let mut location = S3Location::new(bucket, prefix);
    location.endpoint = env::var("GHOSTSNAP_TEST_S3_ENDPOINT").ok();
    location.region = env::var("GHOSTSNAP_TEST_S3_REGION").ok();

    Some((RepositoryLocation::S3(location), password))
}

#[tokio::test]
async fn test_s3_repository_roundtrip_opt_in() {
    let Some((location, password)) = s3_test_config() else {
        eprintln!("Skipping S3 integration test; set GHOSTSNAP_TEST_S3=1 and S3 env vars");
        return;
    };

    let source_dir = tempdir().unwrap();
    create_test_file(source_dir.path().join("hello.txt"), b"hello from s3");
    create_test_file(source_dir.path().join("nested/world.txt"), b"nested data");

    let repo = Repository::init_at_location(location.clone(), &password)
        .await
        .unwrap();
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    let reopened = Repository::open_at_location(location.clone(), &password)
        .await
        .unwrap();
    match reopened.config().transport.as_ref() {
        Some(RepoTransport::S3(config)) => {
            if let RepositoryLocation::S3(input) = &location {
                assert_eq!(config.bucket, input.bucket);
                assert_eq!(config.prefix, input.prefix);
                assert_eq!(config.endpoint, input.endpoint);
                assert_eq!(config.region, input.region);
            } else {
                panic!("expected s3 repository location");
            }
        }
        _ => panic!("expected persisted s3 transport config"),
    }
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

#[tokio::test]
async fn test_copy_local_to_s3_opt_in() {
    let Some((s3_location, password)) = s3_test_config() else {
        eprintln!("Skipping S3 copy test; set GHOSTSNAP_TEST_S3=1 and S3 env vars");
        return;
    };

    let local_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    create_test_file(source_dir.path().join("copy.txt"), b"copy me to s3");

    let local_repo = Repository::init(local_dir.path(), &password).await.unwrap();
    let snapshot_id = backup_dir(&local_repo, source_dir.path()).await.unwrap();

    let s3_repo = Repository::init_at_location(s3_location.clone(), &password)
        .await
        .unwrap();
    copy_snapshot(&local_repo, &s3_repo, &snapshot_id)
        .await
        .unwrap();

    let reopened = Repository::open_at_location(s3_location, &password)
        .await
        .unwrap();
    let snapshots = reopened.list_snapshots().await.unwrap();
    assert!(snapshots.iter().any(|id| id == &snapshot_id));
}

/// Tests that S3 repository listing works correctly with a prefix.
/// This verifies the fix for the S3 list() semantics bug where
/// prefixed directories were not being stripped correctly.
#[tokio::test]
async fn test_s3_listing_with_prefix_opt_in() {
    let Some((location, password)) = s3_test_config() else {
        eprintln!("Skipping S3 listing test; set GHOSTSNAP_TEST_S3=1 and S3 env vars");
        return;
    };

    // The test config should already have a prefix set
    if let RepositoryLocation::S3(ref s3_loc) = location {
        assert!(
            !s3_loc.prefix.is_empty(),
            "Test requires a non-empty prefix to validate listing"
        );
    }

    let source_dir = tempdir().unwrap();
    create_test_file(
        source_dir.path().join("list-test.txt"),
        b"listing test data",
    );

    let repo = Repository::init_at_location(location.clone(), &password)
        .await
        .unwrap();
    let _snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();

    // Reopen to verify listing works
    let reopened = Repository::open_at_location(location, &password)
        .await
        .unwrap();

    // list_snapshots uses storage.list("snapshots") internally
    let snapshots = reopened.list_snapshots().await.unwrap();
    assert!(!snapshots.is_empty(), "Should find at least one snapshot");

    // list_packs uses storage.list("data") internally
    let packs = reopened.list_packs().await.unwrap();
    assert!(!packs.is_empty(), "Should find at least one pack");
}

/// Tests that S3 repository can be reopened using AWS_ENDPOINT_URL env var.
/// This verifies the fix for the bootstrap issue with S3-compatible providers.
#[tokio::test]
async fn test_s3_reopen_with_endpoint_env_var_opt_in() {
    let Some((location, password)) = s3_test_config() else {
        eprintln!("Skipping S3 endpoint env var test; set GHOSTSNAP_TEST_S3=1 and S3 env vars");
        return;
    };

    let (endpoint, bucket, prefix) = match &location {
        RepositoryLocation::S3(s3_loc) => {
            let Some(ref endpoint) = s3_loc.endpoint else {
                eprintln!("Skipping test; requires GHOSTSNAP_TEST_S3_ENDPOINT to be set");
                return;
            };
            (
                endpoint.clone(),
                s3_loc.bucket.clone(),
                s3_loc.prefix.clone(),
            )
        }
        _ => return,
    };

    let source_dir = tempdir().unwrap();
    create_test_file(source_dir.path().join("env-test.txt"), b"env var test data");

    // Initialize with full location including endpoint
    let repo = Repository::init_at_location(location.clone(), &password)
        .await
        .unwrap();
    let snapshot_id = backup_dir(&repo, source_dir.path()).await.unwrap();
    drop(repo);

    // Now try to reopen using only bucket/prefix (endpoint from env var)
    // SAFETY: This test sets an env var for S3 endpoint bootstrap testing.
    // The test is opt-in and runs when GHOSTSNAP_TEST_S3=1 is set.
    unsafe {
        std::env::set_var("AWS_ENDPOINT_URL", &endpoint);
    }

    let minimal_location = RepositoryLocation::S3(S3Location::new(bucket, prefix));
    let reopened = Repository::open_at_location(minimal_location, &password)
        .await
        .expect("Should be able to reopen using AWS_ENDPOINT_URL env var");

    let snapshots = reopened.list_snapshots().await.unwrap();
    assert!(snapshots.iter().any(|id| id == &snapshot_id));

    // SAFETY: Cleanup env var
    unsafe {
        std::env::remove_var("AWS_ENDPOINT_URL");
    }
}
