use anyhow::{Result, anyhow};
use clap::Args;
use ghostsnap_core::{NodeType, Repository};
use std::io::{self, Write};

#[derive(Args)]
pub struct DumpCommand {
    #[arg(help = "Snapshot ID (full or short prefix)")]
    snapshot_id: String,

    #[arg(help = "Path to file within snapshot")]
    path: String,
}

impl DumpCommand {
    pub async fn run(&self, cli: &crate::Cli) -> Result<()> {
        let repo_location = crate::commands::parse_repository_location(cli.repo.as_ref())?;

        let password = cli
            .password
            .clone()
            .or_else(|| {
                // For dump, read password from stderr to keep stdout clean
                eprint!("Enter repository password: ");
                io::stderr().flush().ok()?;
                rpassword::read_password().ok()
            })
            .ok_or_else(|| anyhow!("Password required"))?;

        let repo = Repository::open_at_location(repo_location, &password).await?;

        // Resolve snapshot ID
        let full_snapshot_id = self.resolve_snapshot_id(&repo, &self.snapshot_id).await?;
        let snapshot = repo.load_snapshot(&full_snapshot_id).await?;
        let tree = repo.load_tree(&snapshot.tree).await?;

        // Find the file
        let node = tree
            .nodes
            .iter()
            .find(|n| n.name == self.path || n.name == self.path.trim_start_matches('/'))
            .ok_or_else(|| anyhow!("File not found in snapshot: {}", self.path))?;

        // Resolve hardlink target if this is a hardlink
        let resolved_node = if let Some(ref target_path) = node.hardlink_target {
            // This is a hardlink - find the original file and use its chunks
            tree.nodes
                .iter()
                .find(|n| n.name == *target_path)
                .ok_or_else(|| {
                    anyhow!(
                        "Hardlink target not found in snapshot: {} -> {}",
                        self.path,
                        target_path
                    )
                })?
        } else {
            node
        };

        // Check it's a file
        if !matches!(resolved_node.node_type, NodeType::File) {
            if matches!(resolved_node.node_type, NodeType::Symlink) {
                // For symlinks, output the target
                if let Some(ref target) = resolved_node.link_target {
                    print!("{}", target);
                    return Ok(());
                }
            }
            return Err(anyhow!(
                "Path is not a file: {} (type: {:?})",
                self.path,
                resolved_node.node_type
            ));
        }

        // Read and output file contents using resolved node's chunks
        let stdout = io::stdout();
        let mut handle = stdout.lock();

        for chunk_ref in &resolved_node.chunks {
            let chunk_data = repo.load_chunk(&chunk_ref.id).await?;
            handle.write_all(&chunk_data)?;
        }

        handle.flush()?;

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
