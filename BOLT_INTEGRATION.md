# Bolt Integration with Ghostsnap & Restic

## Overview

Bolt integrates with **ghostsnap** (a Rust-based restic-like backup tool) and **restic** to provide comprehensive backup capabilities that complement Bolt's BTRFS/ZFS snapshot system. This creates a multi-layered backup strategy:

- **Local snapshots** (BTRFS/ZFS) - Instant filesystem-level snapshots for quick rollbacks
- **Remote backups** (ghostsnap/restic) - Encrypted, deduplicated backups to cloud storage

## Architecture

### Backup Strategy Layers

```
┌─────────────────────────────────────────────────────────────────┐
│                    Bolt Backup Strategy                        │
├─────────────────────────────────────────────────────────────────┤
│  Layer 1: Local Snapshots (BTRFS/ZFS)                         │
│  • Instant snapshots for quick recovery                        │
│  • Filesystem-level rollbacks                                  │
│  • Low overhead, high frequency                                │
├─────────────────────────────────────────────────────────────────┤
│  Layer 2: Remote Backups (ghostsnap/restic)                   │
│  • Encrypted, deduplicated backups                            │
│  • Cloud storage (S3, Azure, etc.)                            │
│  • Cross-system recovery                                       │
│  • Long-term retention                                         │
└─────────────────────────────────────────────────────────────────┘
```

### Integration Points

1. **Snapshot Triggers** - Automatically trigger backups after snapshots
2. **Container Data** - Backup container volumes and data
3. **Configuration** - Backup Bolt configurations and Boltfiles
4. **Orchestration** - Coordinate snapshot + backup workflows

## Configuration

### Boltfile.toml Integration

```toml
project = "my-project"

# Existing snapshot configuration
[snapshots]
enabled = true
filesystem = "auto"

[snapshots.retention]
keep_daily = 7
keep_weekly = 4

# NEW: Backup integration
[backups]
enabled = true
tool = "ghostsnap"  # or "restic"
repository = "s3:backup-bucket/bolt-backups"

[backups.ghostsnap]
# Ghostsnap-specific configuration
password_file = "/etc/bolt/backup-password"
exclude_patterns = [
    "*.tmp",
    "*.log",
    "/var/cache/*",
    "/tmp/*"
]

# Backup triggers
[backups.triggers]
after_snapshot = true              # Backup after creating snapshots
daily = "03:00"                   # Daily backup at 3 AM
before_surge_operations = true     # Backup before surge up/down
on_container_changes = true        # Backup when containers change

# Backup retention (independent of snapshot retention)
[backups.retention]
keep_daily = 30      # 30 days of daily backups
keep_weekly = 12     # 12 weeks of weekly backups
keep_monthly = 12    # 12 months of monthly backups
keep_yearly = 5      # 5 years of yearly backups

# What to backup
[backups.include]
container_volumes = true           # Backup all container volumes
bolt_config = true                # Backup Bolt configuration
system_config = [                 # System configuration paths
    "/etc/bolt",
    "/var/lib/bolt",
    "/home/*/.config/bolt"
]
custom_paths = [                  # Custom paths to backup
    "/opt/games",
    "/home/user/projects"
]

# Storage backends
[backups.storage]
primary = "s3:minio/bolt-backups"
secondary = "azure:backup-account/bolt-container"  # Optional secondary backup
```

### Environment Variables

```bash
# Backup tool configuration
export BOLT_BACKUP_TOOL=ghostsnap
export BOLT_BACKUP_REPOSITORY="s3:backup-bucket/bolt-backups"
export BOLT_BACKUP_PASSWORD_FILE="/etc/bolt/backup-password"

# Ghostsnap specific
export GHOSTSNAP_PASSWORD="your-backup-password"
export AWS_ACCESS_KEY_ID="your-access-key"
export AWS_SECRET_ACCESS_KEY="your-secret-key"

# Restic specific (if using restic)
export RESTIC_PASSWORD="your-backup-password"
export RESTIC_REPOSITORY="s3:backup-bucket/bolt-backups"
```

## CLI Commands

### Backup Management

```bash
# Initialize backup repository
bolt backup init s3:backup-bucket/bolt-backups

# Create manual backup
bolt backup create --name "manual-backup-$(date +%Y%m%d)"

# List backups
bolt backup list
bolt backup ls  # alias

# Show backup details
bolt backup show backup-id

# Restore from backup
bolt backup restore backup-id --target /restore/path

# Restore specific files
bolt backup restore backup-id --include "/var/lib/bolt/*" --target /restore

# Check backup integrity
bolt backup check

# Cleanup old backups (apply retention policy)
bolt backup forget --dry-run
bolt backup forget --apply

# Show backup statistics
bolt backup stats
```

### Combined Snapshot + Backup Operations

```bash
# Create snapshot and backup in one command
bolt snapshot-backup create --name "deployment-$(date +%Y%m%d)"

# Snapshot + backup before surge operations
bolt surge up --backup  # Automatically creates snapshot and backup

# Emergency restore (snapshot + backup restore)
bolt emergency-restore backup-id
```

### Backup Configuration

```bash
# Show backup configuration
bolt backup config

# Test backup connectivity
bolt backup test-connection

# Enable/disable automatic backups
bolt backup auto enable
bolt backup auto disable
bolt backup auto status
```

## Integration Workflows

### Daily Operations

```bash
# Morning backup check
bolt backup stats
bolt snapshot list --today

# Deploy with backup protection
bolt snapshot create --name "before-deploy"
bolt surge up
bolt backup create --name "post-deploy-$(date +%Y%m%d)"
```

### Emergency Recovery

```bash
# Quick recovery (local snapshot)
bolt snapshot rollback stable-config

# Full system recovery (remote backup)
bolt backup restore latest --target /
# or
bolt emergency-restore latest
```

### Maintenance

```bash
# Weekly maintenance
bolt snapshot cleanup --force
bolt backup forget --apply
bolt backup check

# Monthly verification
bolt backup restore latest --verify-only
bolt snapshot config --health
```

## Backup Tool Comparison

### Ghostsnap vs Restic

| Feature | Ghostsnap | Restic |
|---------|-----------|--------|
| **Language** | Rust | Go |
| **Performance** | Faster (Rust) | Fast |
| **Memory Usage** | Lower | Higher |
| **Maturity** | Newer | Mature |
| **Backends** | S3, Azure | Many (S3, Azure, GCS, local, etc.) |
| **Encryption** | ChaCha20-Poly1305, AES-GCM | AES-256 |
| **Deduplication** | Yes | Yes |
| **Compression** | Yes | Yes |
| **Bolt Integration** | Native | External |

### When to Use Each

**Use Ghostsnap when:**
- Performance is critical
- You prefer Rust ecosystem
- You want lighter resource usage
- You're using S3/Azure backends

**Use Restic when:**
- You need maximum compatibility
- You use diverse storage backends
- You want mature, battle-tested tool
- You need advanced features

## Security Considerations

### Encryption

```toml
[backups.security]
encryption = "chacha20poly1305"  # or "aes256-gcm"
key_derivation = "argon2"
password_file = "/etc/bolt/backup-password"
key_file = "/etc/bolt/backup-key"  # Optional key file
```

### Access Control

```bash
# Secure backup password
echo "your-secure-password" | sudo tee /etc/bolt/backup-password
sudo chmod 600 /etc/bolt/backup-password
sudo chown bolt:bolt /etc/bolt/backup-password

# Generate key file
ghostsnap generate-key > /etc/bolt/backup-key
sudo chmod 600 /etc/bolt/backup-key
```

### Network Security

```toml
[backups.storage.s3]
endpoint = "https://s3.amazonaws.com"
use_tls = true
verify_certificates = true
```

## Performance Optimization

### Backup Performance

```toml
[backups.performance]
parallel_uploads = 4              # Parallel upload connections
chunk_size = "8MB"               # Chunk size for uploads
compression = "zstd"             # Compression algorithm
compression_level = 3            # Compression level (1-22)
```

### Network Optimization

```toml
[backups.network]
bandwidth_limit = "10MB/s"       # Upload bandwidth limit
retry_attempts = 3               # Network retry attempts
timeout = "30s"                  # Network timeout
```

### Exclude Patterns

```toml
[backups.exclude]
patterns = [
    # Temporary files
    "*.tmp",
    "*.temp",
    "*~",

    # Logs
    "*.log",
    "/var/log/*",

    # Caches
    "*/cache/*",
    "*/.cache/*",
    "/tmp/*",
    "/var/tmp/*",

    # Container runtime
    "/var/lib/docker/tmp/*",
    "/var/lib/bolt/tmp/*",

    # Gaming caches
    "*/Steam/steamapps/shadercache/*",
    "*/.local/share/Steam/logs/*"
]
```

## Monitoring & Alerting

### Backup Health Checks

```bash
# Check backup health
bolt backup health

# Verify recent backups
bolt backup verify --last-week

# Test restore (dry run)
bolt backup test-restore latest --dry-run
```

### Integration with Monitoring

```bash
# Export backup metrics
bolt backup metrics --format prometheus > /var/lib/bolt/metrics/backup.prom

# JSON export for monitoring
bolt backup status --format json
```

## Storage Backends

### S3-Compatible

```toml
[backups.storage.s3]
endpoint = "https://s3.amazonaws.com"
bucket = "bolt-backups"
region = "us-east-1"
access_key_id = "AKIA..."
secret_access_key = "secret"
```

### Azure Blob Storage

```toml
[backups.storage.azure]
account_name = "backupaccount"
container = "bolt-backups"
# Use environment variables for credentials
```

### MinIO (Self-hosted)

```toml
[backups.storage.minio]
endpoint = "https://minio.example.com"
bucket = "bolt-backups"
access_key = "minioadmin"
secret_key = "minioadmin"
use_tls = true
```

## Example Workflows

### Gaming Setup Protection

```toml
project = "gaming-backup"

[snapshots]
enabled = true

[snapshots.triggers]
before_gaming_setup = true

[backups]
enabled = true
tool = "ghostsnap"
repository = "s3:gaming-backups/bolt"

[backups.include]
container_volumes = true
custom_paths = [
    "/opt/games",
    "/home/user/steam",
    "/home/user/.wine"
]

[backups.triggers]
after_snapshot = true
weekly = "sunday@02:00"

[[snapshots.named_snapshots]]
name = "stable-gaming"
description = "Working gaming configuration"
keep_forever = true

[[backups.named_backups]]
name = "stable-gaming-backup"
description = "Gaming environment backup"
keep_forever = true
```

### Development Environment

```toml
project = "dev-backup"

[backups]
enabled = true
tool = "ghostsnap"

[backups.include]
custom_paths = [
    "/home/dev/projects",
    "/var/lib/bolt",
    "/etc/bolt"
]

[backups.triggers]
daily = "01:00"
before_surge_operations = true

[backups.retention]
keep_daily = 14
keep_weekly = 8
```

### Production Server

```toml
project = "production-backup"

[backups]
enabled = true
tool = "ghostsnap"
repository = "s3:prod-backups/bolt"

[backups.storage]
primary = "s3:primary-backup/bolt"
secondary = "azure:backup-account/bolt-secondary"

[backups.triggers]
daily = "02:00"
before_surge_operations = true
after_snapshot = true

[backups.retention]
keep_daily = 30
keep_weekly = 12
keep_monthly = 24
keep_yearly = 7
```

## Troubleshooting

### Common Issues

#### Backup Authentication

```bash
# Test credentials
bolt backup test-connection

# Verify environment variables
echo $BOLT_BACKUP_PASSWORD_FILE
echo $AWS_ACCESS_KEY_ID
```

#### Storage Issues

```bash
# Check storage quota
bolt backup stats

# Verify connectivity
bolt backup test-connection --verbose

# Check repository integrity
bolt backup check --read-data
```

#### Performance Issues

```bash
# Monitor backup progress
bolt backup create --progress

# Check network bandwidth
bolt backup config --show-network

# Optimize chunk size
bolt backup config set chunk-size 16MB
```

## Migration & Upgrade

### Migrating from Existing Backups

```bash
# Import restic repository
bolt backup import-restic /path/to/restic/repo

# Migrate from other tools
bolt backup import --format tar /path/to/backup.tar

# Verify migration
bolt backup list --verbose
```

### Upgrading Backup Tool

```bash
# Switch from restic to ghostsnap
bolt backup migrate-tool restic ghostsnap

# Update repository format
bolt backup upgrade-repo
```

This integration provides a comprehensive backup strategy that combines Bolt's fast local snapshots with robust remote backups using ghostsnap or restic.