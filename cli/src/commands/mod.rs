pub mod backup;
pub mod check;
pub mod copy;
pub mod diff;
pub mod dump;
pub mod forget;
pub mod init;
pub mod job;
pub mod ls;
pub mod prune;
pub mod restore;
pub mod snapshots;
pub mod stats;

use anyhow::{Result, anyhow};
use ghostsnap_core::storage::RepositoryLocation;

pub fn parse_repository_location(repo: Option<&String>) -> Result<RepositoryLocation> {
    let repo =
        repo.ok_or_else(|| anyhow!("Repository path required (--repo or GHOSTSNAP_REPO)"))?;
    RepositoryLocation::parse(repo).map_err(|e| anyhow!(e.to_string()))
}
