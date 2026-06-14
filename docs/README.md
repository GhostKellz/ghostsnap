# Ghostsnap Documentation

Welcome to the Ghostsnap documentation. This guide covers installation, usage, architecture, and development.

## Quick Navigation

### Getting Started
- [Installation](getting-started/installation.md) - Build and install Ghostsnap
- [Quick Start](getting-started/quickstart.md) - Your first backup in 5 minutes
- [Configuration](getting-started/configuration.md) - Environment variables and config files

### Usage
- [Backup](usage/backup.md) - Creating backups
- [Restore](usage/restore.md) - Restoring from snapshots
- [Snapshots](usage/snapshots.md) - Managing snapshots
- [Repository](usage/repository.md) - Repository operations (check, prune, stats)
- [Jobs](usage/job.md) - Config-driven backup jobs
- [Docker Backup](usage/docker-backup.md) - Backing up Docker containers and volumes

### Backends
- [Local Storage](backends/local.md) - File system backend
- [S3](backends/s3.md) - Amazon S3 and S3-compatible storage (MinIO, Wasabi, B2)
- [Backblaze B2](backends/b2.md) - Backblaze B2 via the `b2:` scheme
- [MinIO](backends/minio.md) - MinIO via the `minio:` scheme
- [SFTP](backends/sftp.md) - Native SSH/SFTP backend
- [Azure Blob](backends/azure.md) - Azure Blob Storage
- [Rclone](backends/rclone.md) - 40+ cloud providers via rclone

### Architecture
- [Overview](architecture/overview.md) - System design
- [Encryption](architecture/encryption.md) - Cryptographic design
- [Chunking](architecture/chunking.md) - Content-defined chunking
- [Pack Files](architecture/packs.md) - Pack file format
- [Index](architecture/index.md) - Chunk indexing system

### Development
- [Building](development/building.md) - Build from source
- [Testing](development/testing.md) - Running tests
- [Contributing](development/contributing.md) - Contribution guidelines

### Security
- [Advisories](advisories/) - Resolved `cargo audit` advisories and how they were fixed

### Operator Guides
- [Website Backup](guides/website-backup.md) - Backing up Nginx/web server hosts
- [Automation](guides/automation.md) - Cron and systemd scheduling

### Example Configurations
- [Website to B2](examples/website-b2.toml) - Website backup to Backblaze B2
- [Docker Compose](examples/docker-compose.toml) - Docker application backup

## Command Reference

| Command | Description |
|---------|-------------|
| `init` | Initialize a new repository |
| `backup` | Create a backup snapshot |
| `restore` | Restore files from a snapshot |
| `snapshots` | List all snapshots |
| `ls` | List files in a snapshot |
| `diff` | Compare two snapshots |
| `check` | Verify repository integrity |
| `prune` | Remove unused data |
| `forget` | Apply retention policies |
| `stats` | Show repository statistics |
| `dump` | Extract single file to stdout |
| `copy` | Copy snapshots between repositories |
| `job` | Run config-driven backup jobs |

## Shipped Backends

| Backend | Status | Notes |
|---------|--------|-------|
| Local filesystem | Shipped | Full support |
| Amazon S3 | Shipped | Full support, SSE encryption |
| S3-compatible | Shipped | MinIO, Wasabi, Backblaze B2 S3 API |
| SFTP | Shipped | Native SSH/SFTP |
| Azure Blob | Shipped | Full support |
| Rclone | Shipped | 40+ providers via rclone CLI |
