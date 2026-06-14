use anyhow::{Result, anyhow};
use chrono::{DateTime, TimeZone, Utc};
use clap::Args;
use ghostsnap_core::{NodeType, Repository};
use std::io::{self, Write};

#[derive(Args)]
pub struct LsCommand {
    #[arg(help = "Snapshot ID (full or short prefix)")]
    snapshot_id: String,

    #[arg(help = "Path within snapshot (optional)")]
    path: Option<String>,

    #[arg(short, long, help = "Long listing format")]
    long: bool,

    #[arg(long, help = "Output in JSON format")]
    json: bool,

    #[arg(short, long, help = "Recursive listing")]
    recursive: bool,
}

impl LsCommand {
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

        // Resolve snapshot ID
        let full_snapshot_id = self.resolve_snapshot_id(&repo, &self.snapshot_id).await?;
        let snapshot = repo.load_snapshot(&full_snapshot_id).await?;
        let tree = repo.load_tree(&snapshot.tree).await?;

        // Filter nodes by path prefix
        let filter_path = self.path.as_deref().unwrap_or("");
        let mut nodes: Vec<_> = tree
            .nodes
            .iter()
            .filter(|node| {
                if filter_path.is_empty() {
                    // No path specified: show only top-level items by default
                    // (or all items if recursive flag is set)
                    if self.recursive {
                        true
                    } else {
                        // Top-level items don't contain '/' in their name
                        !node.name.contains('/')
                    }
                } else if self.recursive {
                    node.name.starts_with(filter_path)
                } else {
                    // Non-recursive: only show direct children
                    if node.name.starts_with(filter_path) {
                        let remainder = node
                            .name
                            .strip_prefix(filter_path)
                            .unwrap_or(&node.name)
                            .trim_start_matches('/');
                        !remainder.contains('/')
                    } else {
                        false
                    }
                }
            })
            .collect();

        // Sort by name
        nodes.sort_by(|a, b| a.name.cmp(&b.name));

        if self.json {
            let entries: Vec<_> = nodes
                .iter()
                .map(|node| {
                    serde_json::json!({
                        "name": node.name,
                        "type": match node.node_type {
                            NodeType::File => "file",
                            NodeType::Directory => "directory",
                            NodeType::Symlink => "symlink",
                        },
                        "size": node.size,
                        "mode": format!("{:o}", node.mode),
                        "uid": node.uid,
                        "gid": node.gid,
                        "mtime": node.mtime,
                        "link_target": node.link_target,
                    })
                })
                .collect();

            println!("{}", serde_json::to_string_pretty(&entries)?);
        } else if self.long {
            for node in &nodes {
                let type_char = match node.node_type {
                    NodeType::File => '-',
                    NodeType::Directory => 'd',
                    NodeType::Symlink => 'l',
                };

                let mode_str = format_mode(node.mode);
                let size_str = if matches!(node.node_type, NodeType::Directory) {
                    "-".to_string()
                } else {
                    format_size(node.size)
                };

                let mtime: DateTime<Utc> = Utc
                    .timestamp_opt(node.mtime, 0)
                    .single()
                    .unwrap_or_else(Utc::now);
                let time_str = mtime.format("%Y-%m-%d %H:%M").to_string();

                let name_str = if let Some(ref target) = node.link_target {
                    format!("{} -> {}", node.name, target)
                } else {
                    node.name.clone()
                };

                println!(
                    "{}{} {:>5} {:>5} {:>8} {} {}",
                    type_char, mode_str, node.uid, node.gid, size_str, time_str, name_str
                );
            }
        } else {
            // Simple listing
            for node in &nodes {
                let suffix = match node.node_type {
                    NodeType::Directory => "/",
                    NodeType::Symlink => "@",
                    NodeType::File => "",
                };
                println!("{}{}", node.name, suffix);
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

fn format_mode(mode: u32) -> String {
    let mut s = String::with_capacity(9);

    // Owner
    s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o100 != 0 { 'x' } else { '-' });

    // Group
    s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o010 != 0 { 'x' } else { '-' });

    // Other
    s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o001 != 0 { 'x' } else { '-' });

    s
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{}", bytes)
    }
}
