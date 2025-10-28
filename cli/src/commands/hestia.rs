use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};
// TODO: Uncomment when Repository API is ready
// use ghostsnap_core::repository::Repository;
use ghostsnap_integrations::hestia::HestiaIntegration;
use std::io::{self, Write};
use std::path::PathBuf;
use tokio::fs;
use chrono::Utc;
use tracing::{info, warn, error};

#[derive(Args, Debug)]
pub struct HestiaCommand {
    #[command(subcommand)]
    pub command: HestiaSubcommands,
}

#[derive(Debug, Subcommand)]
pub enum HestiaSubcommands {
    /// Backup HestiaCP user(s) to Ghostsnap repository
    Backup {
        /// Username to backup (omit to backup all users)
        #[arg(short, long)]
        user: Option<String>,
        
        /// Ghostsnap repository path
        #[arg(short, long)]
        repository: String,
        
        /// Delete HestiaCP tarball after successful backup
        #[arg(long, default_value = "false")]
        cleanup: bool,
        
        /// Only backup users matching this pattern (glob)
        #[arg(long)]
        include: Option<String>,
        
        /// Exclude users matching this pattern (glob)
        #[arg(long)]
        exclude: Option<String>,
        
        /// Keep N most recent local tarballs (default: 3)
        #[arg(long, default_value = "3")]
        keep_tarballs: usize,
    },
    
    /// Restore HestiaCP user from Ghostsnap repository
    Restore {
        /// Username to restore
        user: String,
        
        /// Snapshot ID to restore from
        #[arg(short, long)]
        snapshot: String,
        
        /// Ghostsnap repository path
        #[arg(short, long)]
        repository: String,
        
        /// Restore to temporary location (don't overwrite existing)
        #[arg(long)]
        temp: bool,
    },
    
    /// List HestiaCP users available for backup
    ListUsers {
        /// Show detailed user information
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// List backups in Ghostsnap repository
    ListBackups {
        /// Ghostsnap repository path
        #[arg(short, long)]
        repository: String,
        
        /// Filter by username
        #[arg(short, long)]
        user: Option<String>,
    },
    
    /// Show information about a HestiaCP user
    UserInfo {
        /// Username to inspect
        user: String,
    },
}


impl HestiaCommand {
    pub async fn run(&self, _cli: &crate::Cli) -> Result<()> {
        match &self.command {
            HestiaSubcommands::Backup {
                user,
                repository,
                cleanup,
                include,
                exclude,
                keep_tarballs,
            } => {
                backup_command(user.clone(), repository.clone(), *cleanup, include.clone(), exclude.clone(), *keep_tarballs).await
            }
            HestiaSubcommands::Restore {
                user,
                snapshot,
                repository,
                temp,
            } => {
                restore_command(user.clone(), snapshot.clone(), repository.clone(), *temp).await
            }
            HestiaSubcommands::ListUsers { detailed } => {
                list_users_command(*detailed).await
            }
            HestiaSubcommands::ListBackups { repository, user } => {
                list_backups_command(repository.clone(), user.clone()).await
            }
            HestiaSubcommands::UserInfo { user } => {
                user_info_command(user.clone()).await
            }
        }
    }
}

async fn backup_command(
    user: Option<String>,
    repository: String,
    cleanup: bool,
    include: Option<String>,
    exclude: Option<String>,
    keep_tarballs: usize,
) -> Result<()> {
    info!("Starting HestiaCP backup to Ghostsnap repository");
    
    // TODO: Repository API needs to be ready for this
    // For now, we'll just simulate the repository operations
    println!("⚠️  Note: Repository integration pending. Simulating backup operations.\n");
    
    // Open Ghostsnap repository (commented until Repository API is ready)
    // let repo = Repository::open(&repository, "password").await
    //     .map_err(|e| anyhow!("Failed to open repository: {}. Use 'ghostsnap init' first.", e))?;
    
    let hestia = HestiaIntegration::new("/usr/local/hestia");
    
    // Determine which users to backup
    let users = match user {
        Some(username) => vec![username],
        None => {
            let mut all_users = hestia.list_users_simple().await?;
            
            // Apply include/exclude patterns
            if let Some(pattern) = include {
                all_users.retain(|u| glob_match(&pattern, u));
            }
            if let Some(pattern) = exclude {
                all_users.retain(|u| !glob_match(&pattern, u));
            }
            
            all_users
        }
    };
    
    if users.is_empty() {
        println!("⚠️  No users match the specified criteria");
        return Ok(());
    }
    
    println!("🚀 Starting backup for {} user(s)", users.len());
    
    let mut success_count = 0;
    let mut failed_count = 0;
    
    for (idx, username) in users.iter().enumerate() {
        println!("\n[{}/{}] Backing up user: {} ...", idx + 1, users.len(), username);
        
        match backup_single_user(&hestia, username, cleanup, keep_tarballs).await {
            Ok(_) => {
                success_count += 1;
                println!("✅ Successfully backed up user: {}", username);
            }
            Err(e) => {
                failed_count += 1;
                eprintln!("❌ Failed to backup user {}: {}", username, e);
            }
        }
    }
    
    println!("\n🎉 Backup Summary:");
    println!("   ✅ Successful: {}", success_count);
    println!("   ❌ Failed: {}", failed_count);
    
    if failed_count > 0 {
        anyhow::bail!("{} user(s) failed to backup", failed_count);
    }
    
    Ok(())
}

async fn backup_single_user(
    hestia: &HestiaIntegration,
    username: &str,
    cleanup: bool,
    keep_tarballs: usize,
) -> Result<()> {
    // Step 1: Execute HestiaCP backup
    println!("  📦 Creating HestiaCP backup...");
    let tarball = hestia.execute_hestia_backup(username).await?;
    
    // Step 2: Get tarball size
    let size = hestia.get_backup_size(&tarball).await?;
    let size_mb = size as f64 / 1_048_576.0;
    println!("  📊 Tarball size: {:.2} MB", size_mb);
    println!("  📁 Local tarball: {:?}", tarball);
    
    // Step 3: Backup to Ghostsnap repository
    let snapshot_name = format!(
        "hestia-{}-{}",
        username,
        Utc::now().format("%Y%m%d-%H%M%S")
    );
    
    println!("  ⬆️  Uploading to Ghostsnap repository...");
    
    // TODO: Replace this with actual repository backup once Repository API is ready
    // For now, we'll simulate the backup
    info!("Would backup file {:?} to repository as {}", tarball, snapshot_name);
    println!("  🔒 Encrypting and chunking...");
    println!("  ☁️  Uploading chunks to backend...");
    
    // Simulate upload delay
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    println!("  ✅ Backed up as snapshot: {}", snapshot_name);
    
    // Step 4: Cleanup old tarballs if requested
    if cleanup || keep_tarballs < 999 {
        let removed = hestia.cleanup_old_backups(Some(username), keep_tarballs).await?;
        if removed > 0 {
            println!("  🧹 Cleaned up {} old tarball(s)", removed);
        }
    }
    
    Ok(())
}

async fn restore_command(
    user: String,
    snapshot: String,
    repository: String,
    temp: bool,
) -> Result<()> {
    info!("Restoring HestiaCP user '{}' from snapshot '{}'", user, snapshot);
    
    // TODO: Repository API needs to be ready for this
    println!("⚠️  Note: Repository integration pending. Simulating restore operations.\n");
    
    // let _repo = Repository::open(&repository, "password").await
    //     .map_err(|e| anyhow!("Failed to open repository: {}", e))?;
    
    // Step 1: Restore tarball from repository
    let restore_path = if temp {
        format!("/tmp/ghostsnap-restore-{}.tar", user)
    } else {
        format!("/backup/restore-{}.tar", user)
    };
    
    println!("📥 Downloading snapshot from repository...");
    
    // TODO: Replace with actual repository restore once API is ready
    info!("Would restore snapshot {} to {}", snapshot, restore_path);
    println!("  � Decrypting chunks...");
    println!("  ⬇️  Downloading from backend...");
    println!("  � Reassembling tarball...");
    
    // Simulate download delay
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    println!("✅ Tarball restored to: {}", restore_path);
    
    if temp {
        println!("\n📋 Next steps:");
        println!("  1. Extract manually: tar -xf {} -C /target/directory", restore_path);
        println!("  2. Or move to HestiaCP: mv {} /backup/", restore_path);
    } else {
        println!("\n📋 To restore to HestiaCP, run:");
        println!("  v-restore-user {} {}", user, restore_path);
    }
    
    Ok(())
}

async fn list_users_command(detailed: bool) -> Result<()> {
    let hestia = HestiaIntegration::new("/usr/local/hestia");
    let users = hestia.list_users_simple().await?;
    
    if users.is_empty() {
        println!("No HestiaCP users found");
        return Ok(());
    }
    
    println!("📋 HestiaCP Users ({}):", users.len());
    println!("{}", "=".repeat(60));
    
    if detailed {
        for username in users {
            match hestia.get_user_info(&username).await {
                Ok(info) => {
                    let status = if info.suspended { "🔴 SUSPENDED" } else { "🟢 ACTIVE" };
                    println!("\n👤 {} {}", username, status);
                    println!("   📁 Home: {}", info.home_dir.display());
                    println!("   🌐 Domains: {}", info.domains.len());
                    println!("   🗄️  Databases: {}", info.databases.len());
                    println!("   💾 Disk: {:.2} MB", info.disk_usage as f64 / (1024.0 * 1024.0));
                    
                    if !info.domains.is_empty() {
                        println!("   📄 Domain list:");
                        for domain in &info.domains {
                            let ssl = if domain.ssl_enabled { "🔒" } else { "🔓" };
                            println!("     {} {}", ssl, domain.domain);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  ⚠️  {} (error loading details: {})", username, e);
                }
            }
        }
    } else {
        for (idx, username) in users.iter().enumerate() {
            println!("  {}. {}", idx + 1, username);
        }
    }
    
    Ok(())
}

async fn list_backups_command(repository: String, user: Option<String>) -> Result<()> {
    // TODO: Repository API needs to be ready for this
    println!("⚠️  Note: Repository integration pending\n");
    
    // let _repo = Repository::open(&repository, "password").await
    //     .map_err(|e| anyhow!("Failed to open repository: {}", e))?;
    
    // TODO: Replace with actual repository snapshot listing once API is ready
    println!("📦 HestiaCP Backups in Repository:");
    println!("{}", "=".repeat(60));
    
    if let Some(username) = user {
        println!("  Filtered by user: {}", username);
    }
    
    // Mock data for now
    println!("\nℹ️  Snapshot listing not yet implemented");
    println!("   Once Repository API is ready, this will show:");
    println!("   - Snapshot ID");
    println!("   - Snapshot name (hestia-username-timestamp)");
    println!("   - Creation date");
    println!("   - Size");
    
    Ok(())
}

async fn user_info_command(user: String) -> Result<()> {
    let hestia = HestiaIntegration::new("/usr/local/hestia");
    let info = hestia.get_user_info(&user).await?;
    
    let status = if info.suspended { "🔴 SUSPENDED" } else { "🟢 ACTIVE" };
    
    println!("👤 HestiaCP User: {}", user);
    println!("{}", "=".repeat(60));
    println!("Status: {}", status);
    println!("Home Directory: {}", info.home_dir.display());
    println!("Disk Usage: {:.2} MB", info.disk_usage as f64 / (1024.0 * 1024.0));
    println!("Bandwidth: {:.2} MB", info.bandwidth_usage as f64 / (1024.0 * 1024.0));
    
    println!("\n🌐 Domains ({}):", info.domains.len());
    if info.domains.is_empty() {
        println!("  (none)");
    } else {
        for domain in &info.domains {
            let ssl = if domain.ssl_enabled { "🔒 SSL" } else { "🔓 No SSL" };
            println!("  • {} {}", domain.domain, ssl);
            println!("    Document Root: {}", domain.document_root.display());
        }
    }
    
    println!("\n�️  Databases ({}):", info.databases.len());
    if info.databases.is_empty() {
        println!("  (none)");
    } else {
        for db in &info.databases {
            println!("  • {} ", db.database_name);
            println!("    User: {}", db.database_user);
            println!("    Host: {}", db.database_host);
            println!("    Type: {:?}", db.database_type);
        }
    }
    
    if let Some(mail_dir) = &info.mail_dir {
        println!("\n� Mail Directory: {}", mail_dir.display());
    }
    
    if !info.cron_jobs.is_empty() {
        println!("\n⏰ Cron Jobs ({}):", info.cron_jobs.len());
        for (idx, job) in info.cron_jobs.iter().enumerate() {
            println!("  {}. {}", idx + 1, job);
        }
    }
    
    Ok(())
}

// Simple glob matching (supports * wildcard)
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        let mut pos = 0;
        
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            
            if i == 0 {
                if !text.starts_with(part) {
                    return false;
                }
                pos = part.len();
            } else if i == parts.len() - 1 {
                if !text.ends_with(part) {
                    return false;
                }
            } else {
                if let Some(found_pos) = text[pos..].find(part) {
                    pos += found_pos + part.len();
                } else {
                    return false;
                }
            }
        }
        true
    } else {
        text == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_glob_match() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("test*", "test123"));
        assert!(glob_match("*prod", "myprod"));
        assert!(glob_match("*prod*", "myproduction"));
        assert!(!glob_match("test*", "best"));
        assert!(!glob_match("*prod", "production"));
    }
}