use anyhow::{Result, anyhow};
use chrono::{DateTime, Datelike, Duration, Utc};
use clap::Args;
use ghostsnap_core::{LockManager, LockType, Repository};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

#[derive(Args)]
pub struct ForgetCommand {
    #[arg(long, help = "Keep last N snapshots")]
    keep_last: Option<u32>,

    #[arg(long, help = "Keep daily snapshots for N days")]
    keep_daily: Option<u32>,

    #[arg(long, help = "Keep weekly snapshots for N weeks")]
    keep_weekly: Option<u32>,

    #[arg(long, help = "Keep monthly snapshots for N months")]
    keep_monthly: Option<u32>,

    #[arg(long, help = "Keep yearly snapshots for N years")]
    keep_yearly: Option<u32>,

    #[arg(long, help = "Only consider snapshots with these tags")]
    tag: Vec<String>,

    #[arg(long, help = "Only consider snapshots from this host")]
    host: Option<String>,

    #[arg(long, short = 'n', help = "Dry run - don't actually delete")]
    dry_run: bool,

    #[arg(long, help = "Actually delete snapshots (prune after forget)")]
    prune: bool,
}

#[derive(Debug)]
struct SnapshotInfo {
    id: String,
    time: DateTime<Utc>,
    hostname: String,
    tags: Vec<String>,
}

impl ForgetCommand {
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

        // Acquire exclusive lock for forget operation
        let _lock = if let Some(repo_path) = repo.local_path() {
            let lock_manager = LockManager::new(repo_path);
            Some(lock_manager.acquire(LockType::Exclusive, "forget").await?)
        } else {
            tracing::warn!("Repository locking not supported for remote repositories");
            None
        };

        // Load all snapshots
        let snapshot_ids = repo.list_snapshots().await?;
        let mut snapshots = Vec::new();

        for id in snapshot_ids {
            if let Ok(snapshot) = repo.load_snapshot(&id).await {
                let info = SnapshotInfo {
                    id: id.clone(),
                    time: snapshot.time,
                    hostname: snapshot.hostname,
                    tags: snapshot.tags,
                };
                snapshots.push(info);
            }
        }

        // Filter by host and tags
        let filtered: Vec<_> = snapshots
            .into_iter()
            .filter(|s| {
                if let Some(ref host) = self.host
                    && &s.hostname != host
                {
                    return false;
                }
                if !self.tag.is_empty() && !self.tag.iter().any(|t| s.tags.contains(t)) {
                    return false;
                }
                true
            })
            .collect();

        if filtered.is_empty() {
            println!("No snapshots match the filter criteria");
            return Ok(());
        }

        // Sort by time (newest first)
        let mut sorted = filtered;
        sorted.sort_by_key(|s| std::cmp::Reverse(s.time));

        // Apply retention policies
        let keep_ids = self.apply_retention_policies(&sorted);

        // Determine which to forget
        let forget_ids: Vec<_> = sorted
            .iter()
            .filter(|s| !keep_ids.contains(&s.id))
            .collect();

        // Display results
        println!("Retention policy results:");
        println!();

        println!("Keeping {} snapshots:", keep_ids.len());
        for s in &sorted {
            if keep_ids.contains(&s.id) {
                println!(
                    "  {} {} {}",
                    &s.id[..8],
                    s.time.format("%Y-%m-%d %H:%M:%S"),
                    s.hostname
                );
            }
        }

        println!();
        println!("Forgetting {} snapshots:", forget_ids.len());
        for s in &forget_ids {
            println!(
                "  {} {} {}",
                &s.id[..8],
                s.time.format("%Y-%m-%d %H:%M:%S"),
                s.hostname
            );
        }

        if forget_ids.is_empty() {
            println!();
            println!("Nothing to forget");
            return Ok(());
        }

        if self.dry_run {
            println!();
            println!("Dry run - no snapshots were deleted");
            println!("Run without --dry-run to actually delete");
        } else {
            println!();
            print!("Deleting {} snapshots...", forget_ids.len());
            io::stdout().flush()?;

            for s in &forget_ids {
                repo.delete_snapshot(&s.id).await?;
            }

            println!(" done");

            if self.prune {
                println!();
                println!("Running prune to reclaim disk space...");
                let prune_cmd = super::prune::PruneCommand {
                    dry_run: false,
                    max_unused: None,
                };
                prune_cmd.run(cli).await?;
            }
        }

        Ok(())
    }

    fn apply_retention_policies(&self, snapshots: &[SnapshotInfo]) -> HashSet<String> {
        let mut keep = HashSet::new();
        let now = Utc::now();

        // Keep last N
        if let Some(n) = self.keep_last {
            for s in snapshots.iter().take(n as usize) {
                keep.insert(s.id.clone());
            }
        }

        // Keep daily (one per day for N days)
        if let Some(n) = self.keep_daily {
            let cutoff = now - Duration::days(n as i64);
            let mut seen_days: HashMap<String, &SnapshotInfo> = HashMap::new();

            for s in snapshots {
                if s.time >= cutoff {
                    let day_key = s.time.format("%Y-%m-%d").to_string();
                    seen_days.entry(day_key).or_insert(s);
                }
            }

            for s in seen_days.values() {
                keep.insert(s.id.clone());
            }
        }

        // Keep weekly (one per week for N weeks)
        if let Some(n) = self.keep_weekly {
            let cutoff = now - Duration::weeks(n as i64);
            let mut seen_weeks: HashMap<String, &SnapshotInfo> = HashMap::new();

            for s in snapshots {
                if s.time >= cutoff {
                    let week_key = format!("{}-W{:02}", s.time.year(), s.time.iso_week().week());
                    seen_weeks.entry(week_key).or_insert(s);
                }
            }

            for s in seen_weeks.values() {
                keep.insert(s.id.clone());
            }
        }

        // Keep monthly (one per month for N months)
        if let Some(n) = self.keep_monthly {
            let cutoff = now - Duration::days(n as i64 * 31); // Approximate
            let mut seen_months: HashMap<String, &SnapshotInfo> = HashMap::new();

            for s in snapshots {
                if s.time >= cutoff {
                    let month_key = s.time.format("%Y-%m").to_string();
                    seen_months.entry(month_key).or_insert(s);
                }
            }

            for s in seen_months.values() {
                keep.insert(s.id.clone());
            }
        }

        // Keep yearly (one per year for N years)
        if let Some(n) = self.keep_yearly {
            let cutoff = now - Duration::days(n as i64 * 365); // Approximate
            let mut seen_years: HashMap<String, &SnapshotInfo> = HashMap::new();

            for s in snapshots {
                if s.time >= cutoff {
                    let year_key = s.time.format("%Y").to_string();
                    seen_years.entry(year_key).or_insert(s);
                }
            }

            for s in seen_years.values() {
                keep.insert(s.id.clone());
            }
        }

        // If no policy specified, keep all
        if self.keep_last.is_none()
            && self.keep_daily.is_none()
            && self.keep_weekly.is_none()
            && self.keep_monthly.is_none()
            && self.keep_yearly.is_none()
        {
            for s in snapshots {
                keep.insert(s.id.clone());
            }
        }

        keep
    }
}
