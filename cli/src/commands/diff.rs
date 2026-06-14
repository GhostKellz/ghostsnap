use anyhow::{Result, anyhow};
use clap::Args;
use ghostsnap_core::{ChunkID, NodeType, Repository};
use std::collections::HashMap;
use std::io::{self, Write};

#[derive(Args)]
pub struct DiffCommand {
    #[arg(help = "First snapshot ID")]
    snapshot1: String,

    #[arg(help = "Second snapshot ID")]
    snapshot2: String,

    #[arg(long, help = "Show metadata changes (permissions, ownership)")]
    metadata: bool,

    #[arg(long, help = "Output in JSON format")]
    json: bool,
}

#[derive(Debug, Clone)]
struct FileInfo {
    #[allow(dead_code)]
    name: String,
    node_type: NodeType,
    size: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    mtime: i64,
    chunks: Vec<ChunkID>,
    link_target: Option<String>,
}

#[derive(Debug)]
enum ChangeType {
    Added,
    Removed,
    Modified {
        old_size: u64,
        new_size: u64,
    },
    TypeChanged {
        old_type: NodeType,
        new_type: NodeType,
    },
    MetadataChanged,
}

impl DiffCommand {
    pub async fn run(&self, cli: &crate::Cli) -> Result<()> {
        let repo_location = crate::commands::parse_repository_location(cli.repo.as_ref())?;

        let password = cli
            .password
            .clone()
            .or_else(|| {
                print!("Enter repository password: ");
                io::stdout().flush().ok()?;
                rpassword::read_password().ok()
            })
            .ok_or_else(|| anyhow!("Password required"))?;

        let repo = Repository::open_at_location(repo_location, &password).await?;

        // Resolve snapshot IDs
        let id1 = self.resolve_snapshot_id(&repo, &self.snapshot1).await?;
        let id2 = self.resolve_snapshot_id(&repo, &self.snapshot2).await?;

        // Load snapshots and trees
        let snapshot1 = repo.load_snapshot(&id1).await?;
        let snapshot2 = repo.load_snapshot(&id2).await?;

        let tree1 = repo.load_tree(&snapshot1.tree).await?;
        let tree2 = repo.load_tree(&snapshot2.tree).await?;

        // Build file maps
        let files1: HashMap<String, FileInfo> = tree1
            .nodes
            .iter()
            .map(|n| {
                (
                    n.name.clone(),
                    FileInfo {
                        name: n.name.clone(),
                        node_type: n.node_type.clone(),
                        size: n.size,
                        mode: n.mode,
                        uid: n.uid,
                        gid: n.gid,
                        mtime: n.mtime,
                        chunks: n.chunks.iter().map(|c| c.id).collect(),
                        link_target: n.link_target.clone(),
                    },
                )
            })
            .collect();

        let files2: HashMap<String, FileInfo> = tree2
            .nodes
            .iter()
            .map(|n| {
                (
                    n.name.clone(),
                    FileInfo {
                        name: n.name.clone(),
                        node_type: n.node_type.clone(),
                        size: n.size,
                        mode: n.mode,
                        uid: n.uid,
                        gid: n.gid,
                        mtime: n.mtime,
                        chunks: n.chunks.iter().map(|c| c.id).collect(),
                        link_target: n.link_target.clone(),
                    },
                )
            })
            .collect();

        // Find changes
        let mut changes: Vec<(String, ChangeType)> = Vec::new();

        // Added files (in snapshot2 but not snapshot1)
        for name in files2.keys() {
            if !files1.contains_key(name) {
                changes.push((name.clone(), ChangeType::Added));
            }
        }

        // Removed files (in snapshot1 but not snapshot2)
        for name in files1.keys() {
            if !files2.contains_key(name) {
                changes.push((name.clone(), ChangeType::Removed));
            }
        }

        // Modified files (in both but different)
        for (name, info1) in &files1 {
            if let Some(info2) = files2.get(name) {
                // Type changed?
                if info1.node_type != info2.node_type {
                    changes.push((
                        name.clone(),
                        ChangeType::TypeChanged {
                            old_type: info1.node_type.clone(),
                            new_type: info2.node_type.clone(),
                        },
                    ));
                    continue;
                }

                // Content changed?
                if info1.chunks != info2.chunks {
                    changes.push((
                        name.clone(),
                        ChangeType::Modified {
                            old_size: info1.size,
                            new_size: info2.size,
                        },
                    ));
                    continue;
                }

                // Symlink target changed?
                if info1.link_target != info2.link_target {
                    changes.push((
                        name.clone(),
                        ChangeType::Modified {
                            old_size: info1.size,
                            new_size: info2.size,
                        },
                    ));
                    continue;
                }

                // Metadata changed?
                if self.metadata
                    && (info1.mode != info2.mode
                        || info1.uid != info2.uid
                        || info1.gid != info2.gid
                        || info1.mtime != info2.mtime)
                {
                    changes.push((name.clone(), ChangeType::MetadataChanged));
                }
            }
        }

        // Sort changes by name
        changes.sort_by(|a, b| a.0.cmp(&b.0));

        // Output
        if self.json {
            let json_changes: Vec<_> = changes
                .iter()
                .map(|(name, change)| match change {
                    ChangeType::Added => serde_json::json!({
                        "path": name,
                        "change": "added",
                    }),
                    ChangeType::Removed => serde_json::json!({
                        "path": name,
                        "change": "removed",
                    }),
                    ChangeType::Modified { old_size, new_size } => serde_json::json!({
                        "path": name,
                        "change": "modified",
                        "old_size": old_size,
                        "new_size": new_size,
                    }),
                    ChangeType::TypeChanged { old_type, new_type } => serde_json::json!({
                        "path": name,
                        "change": "type_changed",
                        "old_type": format!("{:?}", old_type),
                        "new_type": format!("{:?}", new_type),
                    }),
                    ChangeType::MetadataChanged => serde_json::json!({
                        "path": name,
                        "change": "metadata",
                    }),
                })
                .collect();

            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "snapshot1": &id1[..8],
                    "snapshot2": &id2[..8],
                    "changes": json_changes,
                }))?
            );
        } else {
            println!("Comparing snapshots:");
            println!(
                "  {} ({})",
                &id1[..8],
                snapshot1.time.format("%Y-%m-%d %H:%M:%S")
            );
            println!(
                "  {} ({})",
                &id2[..8],
                snapshot2.time.format("%Y-%m-%d %H:%M:%S")
            );
            println!();

            if changes.is_empty() {
                println!("No differences found");
            } else {
                let added = changes
                    .iter()
                    .filter(|(_, c)| matches!(c, ChangeType::Added))
                    .count();
                let removed = changes
                    .iter()
                    .filter(|(_, c)| matches!(c, ChangeType::Removed))
                    .count();
                let modified = changes
                    .iter()
                    .filter(|(_, c)| matches!(c, ChangeType::Modified { .. }))
                    .count();
                let type_changed = changes
                    .iter()
                    .filter(|(_, c)| matches!(c, ChangeType::TypeChanged { .. }))
                    .count();
                let metadata = changes
                    .iter()
                    .filter(|(_, c)| matches!(c, ChangeType::MetadataChanged))
                    .count();

                println!(
                    "Summary: {} added, {} removed, {} modified",
                    added, removed, modified
                );
                if type_changed > 0 {
                    println!("         {} type changed", type_changed);
                }
                if metadata > 0 {
                    println!("         {} metadata changed", metadata);
                }
                println!();

                for (name, change) in &changes {
                    match change {
                        ChangeType::Added => println!("+ {}", name),
                        ChangeType::Removed => println!("- {}", name),
                        ChangeType::Modified { old_size, new_size } => {
                            if old_size != new_size {
                                println!("M {} ({} -> {} bytes)", name, old_size, new_size);
                            } else {
                                println!("M {}", name);
                            }
                        }
                        ChangeType::TypeChanged { old_type, new_type } => {
                            println!("T {} ({:?} -> {:?})", name, old_type, new_type);
                        }
                        ChangeType::MetadataChanged => println!("m {}", name),
                    }
                }
            }
        }

        Ok(())
    }

    async fn resolve_snapshot_id(&self, repo: &Repository, snapshot_id: &str) -> Result<String> {
        if snapshot_id.len() >= 36 {
            return Ok(snapshot_id.to_string());
        }

        let all_snapshots = repo.list_snapshots().await?;
        let matches: Vec<_> = all_snapshots
            .iter()
            .filter(|id| id.starts_with(snapshot_id))
            .collect();

        match matches.len() {
            0 => Err(anyhow!(
                "No snapshot found with ID starting with '{}'",
                snapshot_id
            )),
            1 => Ok(matches[0].clone()),
            _ => Err(anyhow!(
                "Ambiguous snapshot ID '{}' - matches {} snapshots",
                snapshot_id,
                matches.len()
            )),
        }
    }
}
