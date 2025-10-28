use ghostsnap_core::{Result, Error};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;
use tracing::{info, warn, debug, error};
use chrono::{DateTime, Utc};
use regex::Regex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HestiaIntegration {
    pub hestia_path: PathBuf,
    pub backup_path: PathBuf,
    pub include_system_files: bool,
    pub include_user_data: bool,
    pub include_databases: bool,
    pub include_mail: bool,
    pub exclude_cache: bool,
    pub compress_backups: bool,
    pub mysql_credentials: Option<MySQLCredentials>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySQLCredentials {
    pub host: String,
    pub port: u16,
    pub root_user: String,
    pub root_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HestiaUser {
    pub username: String,
    pub home_dir: PathBuf,
    pub mail_dir: Option<PathBuf>,
    pub cron_jobs: Vec<String>,
    pub domains: Vec<HestiaDomain>,
    pub databases: Vec<HestiaDatabase>,
    pub suspended: bool,
    pub disk_usage: u64,
    pub bandwidth_usage: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HestiaDomain {
    pub domain: String,
    pub username: String,
    pub document_root: PathBuf,
    pub ssl_enabled: bool,
    pub ssl_cert_path: Option<PathBuf>,
    pub nginx_config: Option<PathBuf>,
    pub apache_config: Option<PathBuf>,
    pub access_log: Option<PathBuf>,
    pub error_log: Option<PathBuf>,
    pub subdirectories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HestiaDatabase {
    pub database_name: String,
    pub database_user: String,
    pub database_host: String,
    pub database_type: DatabaseType,
    pub size_mb: Option<f64>,
    pub charset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseType {
    MySQL,
    MariaDB,
    PostgreSQL,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub backup_id: String,
    pub timestamp: DateTime<Utc>,
    pub hestia_version: String,
    pub users: Vec<String>,
    pub domains: Vec<String>,
    pub databases: Vec<String>,
    pub system_config_included: bool,
    pub total_size_bytes: u64,
    pub backup_duration_seconds: u64,
}

impl HestiaIntegration {
    pub fn new<P: AsRef<Path>>(hestia_path: P) -> Self {
        Self {
            hestia_path: hestia_path.as_ref().to_path_buf(),
            backup_path: PathBuf::from("/backup/hestia"),
            include_system_files: true,
            include_user_data: true,
            include_databases: true,
            include_mail: true,
            exclude_cache: true,
            compress_backups: true,
            mysql_credentials: None,
        }
    }
    
    pub fn default() -> Self {
        Self::new("/usr/local/hestia")
    }
    
    pub fn with_mysql_credentials(mut self, host: String, user: String, password: Option<String>) -> Self {
        self.mysql_credentials = Some(MySQLCredentials {
            host,
            port: 3306,
            root_user: user,
            root_password: password,
        });
        self
    }
    
    pub fn set_backup_options(mut self, 
        include_system: bool, 
        include_users: bool, 
        include_databases: bool, 
        include_mail: bool
    ) -> Self {
        self.include_system_files = include_system;
        self.include_user_data = include_users;
        self.include_databases = include_databases;
        self.include_mail = include_mail;
        self
    }
    
    pub async fn get_hestia_version(&self) -> Result<String> {
        let version_path = self.hestia_path.join("conf/hestia.conf");
        if let Ok(content) = fs::read_to_string(&version_path).await {
            for line in content.lines() {
                if line.starts_with("VERSION=") {
                    return Ok(line.replace("VERSION=", "").trim_matches('"').to_string());
                }
            }
        }
        Ok("unknown".to_string())
    }
    
    pub async fn discover_users(&self) -> Result<Vec<HestiaUser>> {
        let users_path = self.hestia_path.join("data/users");
        let mut users = Vec::new();
        
        if !users_path.exists() {
            return Ok(users);
        }
        
        let mut user_entries = fs::read_dir(&users_path).await?;
        while let Some(user_entry) = user_entries.next_entry().await? {
            let username = user_entry.file_name().to_string_lossy().to_string();
            
            if username == "admin" || username.starts_with('.') {
                continue; // Skip admin and hidden files
            }
            
            let user = self.parse_user_config(&username).await?;
            users.push(user);
        }
        
        Ok(users)
    }
    
    async fn parse_user_config(&self, username: &str) -> Result<HestiaUser> {
        let user_config_path = self.hestia_path.join(format!("data/users/{}/user.conf", username));
        let domains = self.discover_user_domains(username).await?;
        let databases = self.discover_user_databases(username).await?;
        
        let home_dir = PathBuf::from(format!("/home/{}", username));
        let mail_dir = if self.hestia_path.join(format!("data/users/{}/mail", username)).exists() {
            Some(PathBuf::from(format!("/home/{}/mail", username)))
        } else {
            None
        };
        
        // Parse user configuration for additional details
        let (suspended, disk_usage, bandwidth_usage) = if user_config_path.exists() {
            self.parse_user_stats(&user_config_path).await.unwrap_or((false, 0, 0))
        } else {
            (false, 0, 0)
        };
        
        let cron_jobs = self.get_user_cron_jobs(username).await?;
        
        Ok(HestiaUser {
            username: username.to_string(),
            home_dir,
            mail_dir,
            cron_jobs,
            domains,
            databases,
            suspended,
            disk_usage,
            bandwidth_usage,
        })
    }
    
    async fn parse_user_stats(&self, config_path: &Path) -> Result<(bool, u64, u64)> {
        let content = fs::read_to_string(config_path).await?;
        let mut suspended = false;
        let mut disk_usage = 0;
        let mut bandwidth_usage = 0;
        
        for line in content.lines() {
            if line.starts_with("SUSPENDED=") {
                suspended = line.contains("yes");
            } else if line.starts_with("U_DISK=") {
                if let Ok(usage) = line.replace("U_DISK=", "").trim_matches('\'').parse::<u64>() {
                    disk_usage = usage * 1024 * 1024; // Convert MB to bytes
                }
            } else if line.starts_with("U_BANDWIDTH=") {
                if let Ok(usage) = line.replace("U_BANDWIDTH=", "").trim_matches('\'').parse::<u64>() {
                    bandwidth_usage = usage * 1024 * 1024; // Convert MB to bytes
                }
            }
        }
        
        Ok((suspended, disk_usage, bandwidth_usage))
    }
    
    async fn discover_user_domains(&self, username: &str) -> Result<Vec<HestiaDomain>> {
        let domains_path = self.hestia_path.join(format!("data/users/{}/domains", username));
        let mut domains = Vec::new();
        
        if !domains_path.exists() {
            return Ok(domains);
        }
        
        let mut domain_entries = fs::read_dir(&domains_path).await?;
        while let Some(domain_entry) = domain_entries.next_entry().await? {
            let domain_name = domain_entry.file_name().to_string_lossy().to_string();
            let domain_config_path = domain_entry.path().join("domain.conf");
            
            if domain_config_path.exists() {
                let domain = self.parse_domain_config(username, &domain_name, &domain_config_path).await?;
                domains.push(domain);
            }
        }
        
        Ok(domains)
    }
    
    async fn parse_domain_config(&self, username: &str, domain_name: &str, config_path: &Path) -> Result<HestiaDomain> {
        let content = fs::read_to_string(config_path).await?;
        let document_root = PathBuf::from(format!("/home/{}/web/{}/public_html", username, domain_name));
        
        let mut ssl_enabled = false;
        let mut nginx_config = None;
        let mut apache_config = None;
        
        for line in content.lines() {
            if line.contains("SSL=") && line.contains("yes") {
                ssl_enabled = true;
            }
        }
        
        // Check for config files
        let nginx_conf_path = self.hestia_path.join(format!("data/users/{}/conf/web/nginx.{}.conf", username, domain_name));
        if nginx_conf_path.exists() {
            nginx_config = Some(nginx_conf_path);
        }
        
        let apache_conf_path = self.hestia_path.join(format!("data/users/{}/conf/web/apache2.{}.conf", username, domain_name));
        if apache_conf_path.exists() {
            apache_config = Some(apache_conf_path);
        }
        
        Ok(HestiaDomain {
            domain: domain_name.to_string(),
            username: username.to_string(),
            document_root,
            ssl_enabled,
            ssl_cert_path: if ssl_enabled {
                Some(PathBuf::from(format!("/usr/local/hestia/ssl/{}", domain_name)))
            } else {
                None
            },
            nginx_config,
            apache_config,
            access_log: Some(PathBuf::from(format!("/var/log/apache2/domains/{}.log", domain_name))),
            error_log: Some(PathBuf::from(format!("/var/log/apache2/domains/{}.error.log", domain_name))),
            subdirectories: vec![], // Would need deeper scanning
        })
    }
    
    async fn discover_user_databases(&self, username: &str) -> Result<Vec<HestiaDatabase>> {
        let db_config_path = self.hestia_path.join(format!("data/users/{}/db.conf", username));
        let mut databases = Vec::new();
        
        if !db_config_path.exists() {
            return Ok(databases);
        }
        
        let content = fs::read_to_string(&db_config_path).await?;
        let mut current_db: Option<HestiaDatabase> = None;
        
        for line in content.lines() {
            if line.starts_with("DB=") {
                if let Some(db) = current_db.take() {
                    databases.push(db);
                }
                
                let db_name = line.replace("DB=", "").trim_matches('\'').to_string();
                current_db = Some(HestiaDatabase {
                    database_name: db_name.clone(),
                    database_user: format!("{}_{}", username, db_name),
                    database_host: "localhost".to_string(),
                    database_type: DatabaseType::MySQL, // Default assumption
                    size_mb: None,
                    charset: None,
                });
            } else if let Some(ref mut db) = current_db {
                if line.starts_with("CHARSET=") {
                    db.charset = Some(line.replace("CHARSET=", "").trim_matches('\'').to_string());
                } else if line.starts_with("HOST=") {
                    db.database_host = line.replace("HOST=", "").trim_matches('\'').to_string();
                }
            }
        }
        
        if let Some(db) = current_db {
            databases.push(db);
        }
        
        Ok(databases)
    }
    
    async fn get_user_cron_jobs(&self, username: &str) -> Result<Vec<String>> {
        let cron_path = self.hestia_path.join(format!("data/users/{}/cron.conf", username));
        let mut jobs = Vec::new();
        
        if cron_path.exists() {
            let content = fs::read_to_string(&cron_path).await?;
            for line in content.lines() {
                if line.starts_with("CMD=") {
                    let cmd = line.replace("CMD=", "").trim_matches('\'').to_string();
                    jobs.push(cmd);
                }
            }
        }
        
        Ok(jobs)
    }
    
    pub async fn backup_user(&self, user: &HestiaUser) -> Result<PathBuf> {
        info!("Starting comprehensive backup for user: {}", user.username);
        
        let backup_dir = self.backup_path.join(format!("{}-{}", user.username, Utc::now().format("%Y%m%d-%H%M%S")));
        fs::create_dir_all(&backup_dir).await?;
        
        let mut backed_up_paths = Vec::new();
        
        // Backup user files
        if self.include_user_data {
            info!("Backing up user files for: {}", user.username);
            let files_backup = self.backup_user_files(user, &backup_dir).await?;
            backed_up_paths.extend(files_backup);
        }
        
        // Backup databases
        if self.include_databases && !user.databases.is_empty() {
            info!("Backing up {} databases for user: {}", user.databases.len(), user.username);
            for database in &user.databases {
                let db_backup = self.backup_database(database, &backup_dir).await?;
                backed_up_paths.push(db_backup);
            }
        }
        
        // Backup mail
        if self.include_mail {
            if let Some(ref mail_dir) = user.mail_dir {
                info!("Backing up mail for user: {}", user.username);
                let mail_backup = self.backup_mail_directory(mail_dir, &backup_dir).await?;
                backed_up_paths.push(mail_backup);
            }
        }
        
        // Create manifest
        let manifest = self.create_backup_manifest(user, &backed_up_paths).await?;
        let manifest_path = backup_dir.join("backup_manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, manifest_json).await?;
        
        info!("Backup completed for user: {} at {}", user.username, backup_dir.display());
        Ok(backup_dir)
    }
    
    async fn backup_user_files(&self, user: &HestiaUser, backup_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut backed_up_paths = Vec::new();
        
        // Backup each domain's files
        for domain in &user.domains {
            if domain.document_root.exists() {
                let domain_backup_dir = backup_dir.join("domains").join(&domain.domain);
                fs::create_dir_all(&domain_backup_dir).await?;
                
                // Copy domain files (this would integrate with Ghostsnap's chunking system)
                info!("Backing up domain files: {} -> {}", 
                    domain.document_root.display(), 
                    domain_backup_dir.display()
                );
                
                backed_up_paths.push(domain.document_root.clone());
                
                // Backup SSL certificates if present
                if let Some(ref ssl_path) = domain.ssl_cert_path {
                    if ssl_path.exists() {
                        backed_up_paths.push(ssl_path.clone());
                    }
                }
                
                // Backup configuration files
                if let Some(ref nginx_config) = domain.nginx_config {
                    if nginx_config.exists() {
                        backed_up_paths.push(nginx_config.clone());
                    }
                }
                
                if let Some(ref apache_config) = domain.apache_config {
                    if apache_config.exists() {
                        backed_up_paths.push(apache_config.clone());
                    }
                }
            }
        }
        
        Ok(backed_up_paths)
    }
    
    async fn backup_database(&self, database: &HestiaDatabase, backup_dir: &Path) -> Result<PathBuf> {
        let db_backup_dir = backup_dir.join("databases");
        fs::create_dir_all(&db_backup_dir).await?;
        
        let backup_file = db_backup_dir.join(format!("{}.sql", database.database_name));
        
        let mysqldump_args = vec![
            "-h", &database.database_host,
            "-u", &database.database_user,
            "--single-transaction",
            "--routines",
            "--triggers",
            &database.database_name,
        ];
        
        info!("Creating database dump: {}", database.database_name);
        
        let mut cmd = Command::new("mysqldump");
        cmd.args(&mysqldump_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        // Add password if available
        if let Some(ref mysql_creds) = self.mysql_credentials {
            if let Some(ref password) = mysql_creds.root_password {
                cmd.arg(format!("-p{}", password));
            }
        }
        
        let output = cmd.output().await
            .map_err(|e| Error::Other(format!("Failed to run mysqldump: {}", e)))?;
        
        if output.status.success() {
            fs::write(&backup_file, &output.stdout).await?;
            info!("Database backup created: {}", backup_file.display());
        } else {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            error!("Database backup failed: {}", error_msg);
            return Err(Error::Other(format!("Database backup failed: {}", error_msg)));
        }
        
        Ok(backup_file)
    }
    
    async fn backup_mail_directory(&self, mail_dir: &Path, backup_dir: &Path) -> Result<PathBuf> {
        let mail_backup_dir = backup_dir.join("mail");
        fs::create_dir_all(&mail_backup_dir).await?;
        
        info!("Backing up mail directory: {}", mail_dir.display());
        
        // This would integrate with Ghostsnap's file backup system
        // For now, just return the path that would be backed up
        Ok(mail_dir.to_path_buf())
    }
    
    async fn create_backup_manifest(&self, user: &HestiaUser, backed_up_paths: &[PathBuf]) -> Result<BackupManifest> {
        let hestia_version = self.get_hestia_version().await?;
        let backup_id = uuid::Uuid::new_v4().to_string();
        
        let domains: Vec<String> = user.domains.iter().map(|d| d.domain.clone()).collect();
        let databases: Vec<String> = user.databases.iter().map(|db| db.database_name.clone()).collect();
        
        // Calculate total size (simplified)
        let mut total_size = 0u64;
        for path in backed_up_paths {
            if let Ok(metadata) = fs::metadata(path).await {
                total_size += metadata.len();
            }
        }
        
        Ok(BackupManifest {
            backup_id,
            timestamp: Utc::now(),
            hestia_version,
            users: vec![user.username.clone()],
            domains,
            databases,
            system_config_included: self.include_system_files,
            total_size_bytes: total_size,
            backup_duration_seconds: 0, // Would be calculated from start time
        })
    }
    
    pub async fn backup_all_users(&self) -> Result<Vec<PathBuf>> {
        let users = self.discover_users().await?;
        let mut backup_paths = Vec::new();
        
        info!("Starting backup for {} users", users.len());
        
        for user in &users {
            if user.suspended {
                warn!("Skipping suspended user: {}", user.username);
                continue;
            }
            
            match self.backup_user(user).await {
                Ok(backup_path) => {
                    backup_paths.push(backup_path);
                    info!("Successfully backed up user: {}", user.username);
                },
                Err(e) => {
                    error!("Failed to backup user {}: {}", user.username, e);
                    // Continue with other users
                }
            }
        }
        
        info!("Completed backup for {} out of {} users", backup_paths.len(), users.len());
        Ok(backup_paths)
    }
    
    pub async fn backup_system_config(&self) -> Result<PathBuf> {
        info!("Backing up HestiaCP system configuration");
        
        let backup_dir = self.backup_path.join(format!("system-config-{}", Utc::now().format("%Y%m%d-%H%M%S")));
        fs::create_dir_all(&backup_dir).await?;
        
        let config_paths = vec![
            self.hestia_path.join("conf"),
            self.hestia_path.join("data/templates"),
            PathBuf::from("/etc/nginx"),
            PathBuf::from("/etc/apache2/sites-available"),
            PathBuf::from("/etc/mysql"),
            PathBuf::from("/etc/php"),
        ];
        
        for config_path in &config_paths {
            if config_path.exists() {
                info!("Backing up config directory: {}", config_path.display());
                // This would integrate with Ghostsnap's backup system
            }
        }
        
        Ok(backup_dir)
    }
    
    // ========== Wrapper Methods for HestiaCP Native Commands ==========
    
    /// Execute HestiaCP's native v-backup-user command
    pub async fn execute_hestia_backup(&self, username: &str) -> Result<PathBuf> {
        info!("Executing HestiaCP native backup for user: {}", username);
        
        // Check if user exists first
        let user_conf = self.hestia_path.join(format!("data/users/{}/user.conf", username));
        if !user_conf.exists() {
            return Err(Error::Other(format!(
                "User '{}' does not exist in HestiaCP", 
                username
            )));
        }
        
        // Execute v-backup-user command
        let output = Command::new("v-backup-user")
            .arg(username)
            .output()
            .await
            .map_err(|e| Error::Other(format!(
                "Failed to execute v-backup-user: {}. Is HestiaCP installed?", 
                e
            )))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Other(format!(
                "HestiaCP backup failed for user '{}': {}", 
                username, 
                stderr
            )));
        }
        
        info!("HestiaCP backup command completed successfully for user: {}", username);
        
        // Wait a moment for filesystem to settle
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        // Find and return the created backup tarball
        self.find_latest_backup_tarball(username).await
    }
    
    /// Find the most recent backup tarball for a user
    async fn find_latest_backup_tarball(&self, username: &str) -> Result<PathBuf> {
        let backup_dir = PathBuf::from("/backup");
        
        if !backup_dir.exists() {
            return Err(Error::Other(
                "Backup directory /backup does not exist".to_string()
            ));
        }
        
        let mut entries = fs::read_dir(&backup_dir).await
            .map_err(|e| Error::Other(format!(
                "Cannot read backup directory: {}", 
                e
            )))?;
        
        let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();
            
            // Match HestiaCP backup pattern: username.YYYY-MM-DD_HH-MM-SS.tar
            if filename_str.starts_with(&format!("{}.", username)) && filename_str.ends_with(".tar") {
                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(modified) = metadata.modified() {
                        match latest {
                            None => latest = Some((path, modified)),
                            Some((_, latest_time)) if modified > latest_time => {
                                latest = Some((path, modified));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        
        latest
            .map(|(path, _)| path)
            .ok_or_else(|| Error::Other(format!(
                "No backup tarball found for user '{}' in /backup/", 
                username
            )))
    }
    
    /// List all HestiaCP users (simple version using filesystem)
    pub async fn list_users_simple(&self) -> Result<Vec<String>> {
        let users_dir = self.hestia_path.join("data/users");
        
        if !users_dir.exists() {
            return Err(Error::Other(
                "HestiaCP users directory not found. Is HestiaCP installed?".to_string()
            ));
        }
        
        let mut entries = fs::read_dir(&users_dir).await?;
        let mut users = Vec::new();
        
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(username) = entry.file_name().to_str() {
                    // Skip hidden directories and certain system entries
                    if !username.starts_with('.') && username != "history" {
                        users.push(username.to_string());
                    }
                }
            }
        }
        
        users.sort();
        Ok(users)
    }
    
    /// Get basic user information
    pub async fn get_user_info(&self, username: &str) -> Result<HestiaUser> {
        // Reuse the existing parse_user_config method
        self.parse_user_config(username).await
    }
    
    /// Clean up old backup tarballs, keeping only the N most recent
    pub async fn cleanup_old_backups(&self, username: Option<&str>, keep_count: usize) -> Result<usize> {
        let backup_dir = PathBuf::from("/backup");
        
        if !backup_dir.exists() {
            return Ok(0);
        }
        
        let mut entries = fs::read_dir(&backup_dir).await?;
        let mut backups: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();
            
            // Match backup tarballs
            let matches = if let Some(user) = username {
                filename_str.starts_with(&format!("{}.", user)) && filename_str.ends_with(".tar")
            } else {
                filename_str.ends_with(".tar")
            };
            
            if matches {
                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(modified) = metadata.modified() {
                        backups.push((path, modified));
                    }
                }
            }
        }
        
        // Sort by modification time (newest first)
        backups.sort_by(|a, b| b.1.cmp(&a.1));
        
        // Remove old backups beyond keep_count
        let mut removed_count = 0;
        for (path, _) in backups.iter().skip(keep_count) {
            match fs::remove_file(path).await {
                Ok(_) => {
                    info!("Removed old backup: {}", path.display());
                    removed_count += 1;
                }
                Err(e) => {
                    warn!("Failed to remove backup {}: {}", path.display(), e);
                }
            }
        }
        
        Ok(removed_count)
    }
    
    /// Get backup tarball size in bytes
    pub async fn get_backup_size(&self, tarball_path: &Path) -> Result<u64> {
        let metadata = fs::metadata(tarball_path).await
            .map_err(|e| Error::Other(format!(
                "Cannot read backup file metadata: {}", 
                e
            )))?;
        
        Ok(metadata.len())
    }
}

use uuid;