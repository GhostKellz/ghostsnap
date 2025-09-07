use anyhow::{anyhow, Result};
use clap::Args;
use ghostsnap_integrations::HestiaIntegration;
use std::io::{self, Write};
use tracing::info;

#[derive(Args)]
pub struct HestiaCommand {
    #[command(subcommand)]
    action: HestiaAction,
}

#[derive(clap::Subcommand)]
enum HestiaAction {
    #[command(about = "Discover all HestiaCP users and sites")]
    Discover {
        #[arg(long, help = "HestiaCP installation path", default_value = "/usr/local/hestia")]
        hestia_path: String,
        
        #[arg(long, help = "Output format (json, table)")]
        format: Option<String>,
        
        #[arg(long, help = "Include suspended users")]
        include_suspended: bool,
    },
    
    #[command(about = "Backup specific user")]
    BackupUser {
        #[arg(help = "Username to backup")]
        username: String,
        
        #[arg(long, help = "HestiaCP installation path", default_value = "/usr/local/hestia")]
        hestia_path: String,
        
        #[arg(long, help = "Backup destination directory")]
        backup_dir: Option<String>,
        
        #[arg(long, help = "MySQL host for database backups")]
        mysql_host: Option<String>,
        
        #[arg(long, help = "MySQL root user")]
        mysql_user: Option<String>,
        
        #[arg(long, help = "MySQL root password")]
        mysql_password: Option<String>,
        
        #[arg(long, help = "Skip database backups")]
        skip_databases: bool,
        
        #[arg(long, help = "Skip mail backups")]
        skip_mail: bool,
        
        #[arg(long, help = "Skip user files")]
        skip_files: bool,
    },
    
    #[command(about = "Backup all HestiaCP users")]
    BackupAll {
        #[arg(long, help = "HestiaCP installation path", default_value = "/usr/local/hestia")]
        hestia_path: String,
        
        #[arg(long, help = "Backup destination directory")]
        backup_dir: Option<String>,
        
        #[arg(long, help = "MySQL host for database backups")]
        mysql_host: Option<String>,
        
        #[arg(long, help = "MySQL root user")]
        mysql_user: Option<String>,
        
        #[arg(long, help = "MySQL root password")]
        mysql_password: Option<String>,
        
        #[arg(long, help = "Skip suspended users")]
        skip_suspended: bool,
        
        #[arg(long, help = "Skip database backups")]
        skip_databases: bool,
        
        #[arg(long, help = "Skip mail backups")]
        skip_mail: bool,
        
        #[arg(long, help = "Skip user files")]
        skip_files: bool,
        
        #[arg(long, help = "Maximum concurrent backups")]
        max_parallel: Option<usize>,
    },
    
    #[command(about = "Backup HestiaCP system configuration")]
    BackupSystem {
        #[arg(long, help = "HestiaCP installation path", default_value = "/usr/local/hestia")]
        hestia_path: String,
        
        #[arg(long, help = "Backup destination directory")]
        backup_dir: Option<String>,
    },
    
    #[command(about = "Show HestiaCP system statistics")]
    Stats {
        #[arg(long, help = "HestiaCP installation path", default_value = "/usr/local/hestia")]
        hestia_path: String,
        
        #[arg(long, help = "Include detailed per-user statistics")]
        detailed: bool,
    },
}

impl HestiaCommand {
    pub async fn run(&self, _cli: &crate::Cli) -> Result<()> {
        match &self.action {
            HestiaAction::Discover { hestia_path, format, include_suspended } => {
                self.discover_users(hestia_path, format, *include_suspended).await
            },
            HestiaAction::BackupUser { 
                username, 
                hestia_path, 
                backup_dir,
                mysql_host,
                mysql_user,
                mysql_password,
                skip_databases,
                skip_mail,
                skip_files,
            } => {
                self.backup_user(
                    username, 
                    hestia_path, 
                    backup_dir,
                    mysql_host,
                    mysql_user,
                    mysql_password,
                    *skip_databases,
                    *skip_mail,
                    *skip_files,
                ).await
            },
            HestiaAction::BackupAll {
                hestia_path,
                backup_dir,
                mysql_host,
                mysql_user,
                mysql_password,
                skip_suspended,
                skip_databases,
                skip_mail,
                skip_files,
                max_parallel,
            } => {
                self.backup_all_users(
                    hestia_path,
                    backup_dir,
                    mysql_host,
                    mysql_user,
                    mysql_password,
                    *skip_suspended,
                    *skip_databases,
                    *skip_mail,
                    *skip_files,
                    *max_parallel,
                ).await
            },
            HestiaAction::BackupSystem { hestia_path, backup_dir } => {
                self.backup_system_config(hestia_path, backup_dir).await
            },
            HestiaAction::Stats { hestia_path, detailed } => {
                self.show_stats(hestia_path, *detailed).await
            },
        }
    }
    
    async fn discover_users(&self, hestia_path: &str, format: &Option<String>, include_suspended: bool) -> Result<()> {
        info!("Discovering HestiaCP users at: {}", hestia_path);
        
        let hestia = HestiaIntegration::new(hestia_path);
        let users = hestia.discover_users().await?;
        
        let filtered_users: Vec<_> = if include_suspended {
            users
        } else {
            users.into_iter().filter(|u| !u.suspended).collect()
        };
        
        let output_format = format.as_deref().unwrap_or("table");
        
        match output_format {
            "json" => {
                let json_output = serde_json::to_string_pretty(&filtered_users)?;
                println!("{}", json_output);
            },
            "table" => {
                println!("{:<15} {:<8} {:<12} {:<8} {:<10} {:<15}", 
                    "Username", "Domains", "Databases", "Status", "Disk (MB)", "Bandwidth (MB)");
                println!("{}", "-".repeat(80));
                
                for user in &filtered_users {
                    let status = if user.suspended { "SUSPENDED" } else { "ACTIVE" };
                    let disk_mb = user.disk_usage / (1024 * 1024);
                    let bandwidth_mb = user.bandwidth_usage / (1024 * 1024);
                    
                    println!("{:<15} {:<8} {:<12} {:<8} {:<10} {:<15}", 
                        user.username,
                        user.domains.len(),
                        user.databases.len(),
                        status,
                        disk_mb,
                        bandwidth_mb
                    );
                    
                    for domain in &user.domains {
                        let ssl_status = if domain.ssl_enabled { "SSL" } else { "No SSL" };
                        println!("  └─ {} ({})", domain.domain, ssl_status);
                    }
                }
                
                println!("\nTotal users: {}", filtered_users.len());
                println!("Total domains: {}", filtered_users.iter().map(|u| u.domains.len()).sum::<usize>());
                println!("Total databases: {}", filtered_users.iter().map(|u| u.databases.len()).sum::<usize>());
            },
            _ => {
                return Err(anyhow!("Unsupported format: {}. Use 'table' or 'json'", output_format));
            }
        }
        
        Ok(())
    }
    
    async fn backup_user(
        &self,
        username: &str,
        hestia_path: &str,
        backup_dir: &Option<String>,
        mysql_host: &Option<String>,
        mysql_user: &Option<String>,
        mysql_password: &Option<String>,
        skip_databases: bool,
        skip_mail: bool,
        skip_files: bool,
    ) -> Result<()> {
        info!("Starting backup for user: {}", username);
        
        let mut hestia = HestiaIntegration::new(hestia_path);
        
        if let Some(backup_path) = backup_dir {
            hestia.backup_path = backup_path.into();
        }
        
        // Configure MySQL credentials if provided
        if let (Some(host), Some(user)) = (mysql_host, mysql_user) {
            hestia = hestia.with_mysql_credentials(
                host.clone(),
                user.clone(),
                mysql_password.clone(),
            );
        }
        
        // Set backup options
        hestia = hestia.set_backup_options(
            false, // system files not needed for single user
            !skip_files,
            !skip_databases,
            !skip_mail,
        );
        
        // Find the user
        let users = hestia.discover_users().await?;
        let user = users.iter()
            .find(|u| u.username == username)
            .ok_or_else(|| anyhow!("User '{}' not found", username))?;
        
        if user.suspended {
            println!("Warning: User '{}' is suspended", username);
            print!("Continue with backup? (y/N): ");
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().to_lowercase().starts_with('y') {
                println!("Backup cancelled");
                return Ok(());
            }
        }
        
        // Perform backup
        let backup_path = hestia.backup_user(user).await?;
        
        println!("✅ User backup completed successfully!");
        println!("📁 Backup location: {}", backup_path.display());
        println!("👤 User: {}", user.username);
        println!("🌐 Domains: {}", user.domains.len());
        println!("🗄️  Databases: {}", user.databases.len());
        println!("📧 Mail: {}", if user.mail_dir.is_some() { "Yes" } else { "No" });
        
        Ok(())
    }
    
    async fn backup_all_users(
        &self,
        hestia_path: &str,
        backup_dir: &Option<String>,
        mysql_host: &Option<String>,
        mysql_user: &Option<String>,
        mysql_password: &Option<String>,
        skip_suspended: bool,
        skip_databases: bool,
        skip_mail: bool,
        skip_files: bool,
        _max_parallel: Option<usize>,
    ) -> Result<()> {
        info!("Starting backup for all HestiaCP users");
        
        let mut hestia = HestiaIntegration::new(hestia_path);
        
        if let Some(backup_path) = backup_dir {
            hestia.backup_path = backup_path.into();
        }
        
        // Configure MySQL credentials if provided
        if let (Some(host), Some(user)) = (mysql_host, mysql_user) {
            hestia = hestia.with_mysql_credentials(
                host.clone(),
                user.clone(),
                mysql_password.clone(),
            );
        }
        
        // Set backup options
        hestia = hestia.set_backup_options(
            false, // system files handled separately
            !skip_files,
            !skip_databases,
            !skip_mail,
        );
        
        let users = hestia.discover_users().await?;
        let mut successful_backups = 0;
        let mut failed_backups = 0;
        
        println!("🚀 Starting backup for {} users", users.len());
        
        for (i, user) in users.iter().enumerate() {
            if user.suspended && skip_suspended {
                println!("⏭️  Skipping suspended user: {} ({}/{})", user.username, i + 1, users.len());
                continue;
            }
            
            println!("📦 Backing up user: {} ({}/{}) - {} domains, {} databases", 
                user.username, i + 1, users.len(), user.domains.len(), user.databases.len());
            
            match hestia.backup_user(user).await {
                Ok(backup_path) => {
                    successful_backups += 1;
                    println!("✅ Success: {}", backup_path.display());
                },
                Err(e) => {
                    failed_backups += 1;
                    eprintln!("❌ Failed to backup {}: {}", user.username, e);
                }
            }
        }
        
        println!("\n🎉 Backup Summary:");
        println!("✅ Successful: {}", successful_backups);
        println!("❌ Failed: {}", failed_backups);
        println!("📊 Success Rate: {:.1}%", 
            (successful_backups as f64 / (successful_backups + failed_backups) as f64) * 100.0);
        
        Ok(())
    }
    
    async fn backup_system_config(&self, hestia_path: &str, backup_dir: &Option<String>) -> Result<()> {
        info!("Backing up HestiaCP system configuration");
        
        let mut hestia = HestiaIntegration::new(hestia_path);
        
        if let Some(backup_path) = backup_dir {
            hestia.backup_path = backup_path.into();
        }
        
        let backup_path = hestia.backup_system_config().await?;
        
        println!("✅ System configuration backup completed!");
        println!("📁 Backup location: {}", backup_path.display());
        
        Ok(())
    }
    
    async fn show_stats(&self, hestia_path: &str, detailed: bool) -> Result<()> {
        info!("Gathering HestiaCP statistics");
        
        let hestia = HestiaIntegration::new(hestia_path);
        let version = hestia.get_hestia_version().await?;
        let users = hestia.discover_users().await?;
        
        let total_domains: usize = users.iter().map(|u| u.domains.len()).sum();
        let total_databases: usize = users.iter().map(|u| u.databases.len()).sum();
        let total_disk: u64 = users.iter().map(|u| u.disk_usage).sum();
        let total_bandwidth: u64 = users.iter().map(|u| u.bandwidth_usage).sum();
        let suspended_users: usize = users.iter().filter(|u| u.suspended).count();
        let ssl_domains: usize = users.iter()
            .flat_map(|u| &u.domains)
            .filter(|d| d.ssl_enabled)
            .count();
        
        println!("📊 HestiaCP Server Statistics");
        println!("═══════════════════════════════");
        println!("🖥️  HestiaCP Version: {}", version);
        println!("👥 Total Users: {} ({} active, {} suspended)", 
            users.len(), users.len() - suspended_users, suspended_users);
        println!("🌐 Total Domains: {} ({} with SSL)", total_domains, ssl_domains);
        println!("🗄️  Total Databases: {}", total_databases);
        println!("💾 Total Disk Usage: {:.2} GB", total_disk as f64 / (1024.0 * 1024.0 * 1024.0));
        println!("🌍 Total Bandwidth: {:.2} GB", total_bandwidth as f64 / (1024.0 * 1024.0 * 1024.0));
        
        if detailed {
            println!("\n👥 User Details:");
            println!("═════════════════");
            
            for user in &users {
                let status = if user.suspended { "🔴 SUSPENDED" } else { "🟢 ACTIVE" };
                println!("\n👤 {} ({})", user.username, status);
                println!("   🏠 Home: {}", user.home_dir.display());
                println!("   🌐 Domains: {}", user.domains.len());
                println!("   🗄️  Databases: {}", user.databases.len());
                println!("   💾 Disk: {:.2} MB", user.disk_usage as f64 / (1024.0 * 1024.0));
                println!("   🌍 Bandwidth: {:.2} MB", user.bandwidth_usage as f64 / (1024.0 * 1024.0));
                
                if !user.domains.is_empty() {
                    println!("   📄 Domains:");
                    for domain in &user.domains {
                        let ssl = if domain.ssl_enabled { "🔒" } else { "🔓" };
                        println!("     {} {} {}", ssl, domain.domain, domain.document_root.display());
                    }
                }
                
                if !user.databases.is_empty() {
                    println!("   🗄️  Databases:");
                    for db in &user.databases {
                        println!("     {} ({}@{})", db.database_name, db.database_user, db.database_host);
                    }
                }
                
                if !user.cron_jobs.is_empty() {
                    println!("   ⏰ Cron Jobs: {}", user.cron_jobs.len());
                }
            }
        }
        
        Ok(())
    }
}