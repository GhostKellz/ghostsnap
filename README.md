<p align="center">
  <img src="assets/ghostsnap-logo.png" alt="Ghostsnap Logo" width="200" height="200">
</p>

<h1 align="center">Ghostsnap</h1>

<p align="center">
  <strong>Fast, Secure, Deduplicating Backup CLI for Linux</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-B7410E?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black" alt="Linux">
  <img src="https://img.shields.io/badge/BLAKE3-4A154B?style=for-the-badge&logo=hashnode&logoColor=white" alt="BLAKE3">
  <img src="https://img.shields.io/badge/ChaCha20-2E7D32?style=for-the-badge&logo=gnuprivacyguard&logoColor=white" alt="ChaCha20">
  <img src="https://img.shields.io/badge/S3-569A31?style=for-the-badge&logo=amazons3&logoColor=white" alt="S3">
  <img src="https://img.shields.io/badge/Azure-0078D4?style=for-the-badge&logo=microsoftazure&logoColor=white" alt="Azure">
  <img src="https://img.shields.io/badge/SFTP-4D4D4D?style=for-the-badge&logo=openssh&logoColor=white" alt="SFTP">
  <img src="https://img.shields.io/badge/Rclone-0078D7?style=for-the-badge&logo=icloud&logoColor=white" alt="Rclone">
</p>

---

## Overview

**Ghostsnap** is a fast, secure, deduplicating backup tool for Linux systems. Built in Rust for performance and safety, it provides encrypted backups with content-defined chunking for efficient storage.

Inspired by restic, Ghostsnap aims to be a reliable swiss-army knife for backups: back up filesystem paths, store backups locally or in object storage, and support practical self-hosted workflows.

---

## Features

- **Encryption**: ChaCha20-Poly1305 authenticated encryption with Argon2id key derivation
- **Deduplication**: FastCDC content-defined chunking with BLAKE3 hashing
- **Repository targets**: Local filesystem, Amazon S3 (and S3-compatible: Wasabi, Backblaze B2, MinIO), Azure Blob Storage, native SFTP, Rclone (40+ providers)
- **Incremental backups**: Only changed data is stored
- **Snapshot management**: Tag, list, compare, and restore from any point in time
- **Integrity verification**: BLAKE3 checksums on all data
- **Repository maintenance**: `check`, `prune`, `forget`, `copy`, `stats`
- **Config-driven jobs**: TOML-based job definitions with pre/post hooks and retention policies
- **Progress tracking**: Real-time throughput, ETA, and completion status
- **Unix metadata**: Permissions, ownership, hardlinks, symlinks, extended attributes, sparse files

---

## Installation

```bash
# Clone the repository
git clone https://github.com/GhostKellz/ghostsnap
cd ghostsnap

# Build release binary
cargo build --release

# Install (optional)
cp target/x86_64-unknown-linux-gnu/release/ghostsnap /usr/local/bin/
```

Built with Rust (2024 edition). See [docs/getting-started/installation.md](docs/getting-started/installation.md) for details.

---

## Quick Start

### Local Repository

```bash
# Initialize a local repository
ghostsnap init /backup/repo

# Back up a directory
ghostsnap --repo /backup/repo backup ~/documents --tag docs

# List snapshots
ghostsnap --repo /backup/repo snapshots

# Restore a snapshot
ghostsnap --repo /backup/repo restore abc123 --target /restore

# Check repository integrity
ghostsnap --repo /backup/repo check
```

### S3 Repository

```bash
# Initialize an S3 repository
ghostsnap init --backend s3 --bucket my-bucket --prefix backups s3:my-bucket/backups

# Back up to S3
ghostsnap --repo s3:my-bucket/backups backup /data --tag daily

# S3-compatible storage (MinIO, Wasabi, Backblaze B2)
ghostsnap init --backend s3 --bucket my-bucket --endpoint https://s3.wasabisys.com s3:my-bucket
```

### Azure Repository

```bash
# Set Azure credentials
export AZURE_STORAGE_KEY="your-storage-account-key"

# Initialize an Azure repository
ghostsnap init --backend azure --account-name mystorageaccount --container backups

# Back up to Azure
ghostsnap --repo azure:mystorageaccount/backups backup /data --tag daily
```

### SFTP Repository

```bash
# Set SFTP credentials (or use key-based auth via ~/.ssh)
export SFTP_PASSWORD="your-password"

# Initialize an SFTP repository
ghostsnap init --backend sftp sftp:backup@host.example.com:22/backups/repo

# Back up over SFTP
ghostsnap --repo sftp:backup@host.example.com/backups/repo backup /data --tag daily
```

Host keys are verified against `~/.ssh/known_hosts` by default.

### Rclone Repository (40+ Providers)

```bash
# Configure rclone first (one-time setup)
rclone config  # Follow prompts to add a remote

# Initialize repository via rclone
ghostsnap init --backend rclone --remote gdrive --rclone-path backups/ghostsnap

# Back up using rclone
ghostsnap --repo rclone:gdrive/backups/ghostsnap backup /data --tag daily

# Works with any rclone remote: Google Drive, Dropbox, OneDrive, SFTP, etc.
```

### Repository Maintenance

```bash
# Show repository statistics
ghostsnap --repo /backup/repo stats

# List files in a snapshot
ghostsnap --repo /backup/repo ls abc123

# Compare two snapshots
ghostsnap --repo /backup/repo diff abc123 def456

# Apply retention policy (keep last 7 daily, 4 weekly)
ghostsnap --repo /backup/repo forget --keep-daily 7 --keep-weekly 4

# Remove unreferenced data
ghostsnap --repo /backup/repo prune

# Copy snapshots to another repository
ghostsnap --repo /backup/repo copy --repo2 /offsite/backup abc123
```

### Config-Driven Jobs

```bash
# Create a job config (/etc/ghostsnap/jobs.toml)
# Run a backup job with pre/post hooks and retention
ghostsnap job --config /etc/ghostsnap/jobs.toml run website-b2

# List configured jobs
ghostsnap job --config /etc/ghostsnap/jobs.toml list

# Validate job configuration
ghostsnap job --config /etc/ghostsnap/jobs.toml validate website-b2
```

See [docs/guides/website-backup.md](docs/guides/website-backup.md) for complete examples.

---

## Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize a new repository |
| `backup` | Create a new backup snapshot |
| `restore` | Restore files from a snapshot |
| `snapshots` | List snapshots in repository |
| `ls` | List files in a snapshot |
| `diff` | Compare two snapshots |
| `dump` | Extract a file to stdout |
| `check` | Verify repository integrity |
| `stats` | Show repository statistics |
| `forget` | Apply retention policies |
| `prune` | Remove unreferenced data |
| `copy` | Copy snapshots between repositories |
| `job` | Run config-driven backup jobs |

---

## Status

**Core functionality is complete and tested:**
- Local, S3, Azure Blob Storage, native SFTP, and Rclone repository targets
- Full command set for backup, restore, and maintenance
- Encryption, deduplication, and integrity verification
- Repository locking for local repositories (see [locking docs](docs/usage/repository.md#repository-locking) for remote guidance)
- S3-compatible storage: AWS, Wasabi, Backblaze B2, MinIO
- Native SFTP with `known_hosts` host-key verification
- Rclone: Google Drive, Dropbox, OneDrive, SFTP, and 40+ more providers

**In development:**
- Remote repository locking (single-writer patterns documented, distributed locking pending)

See [SECURITY.md](SECURITY.md) for security details.

---

## Documentation

- [Getting Started](docs/getting-started/)
- [Usage Guide](docs/usage/)
- [Architecture](docs/architecture/)
- [Storage Backends](docs/backends/)
- [Development](docs/development/)

---

## Contributing

Contributions are welcome. See [docs/development/contributing.md](docs/development/contributing.md).

---

## License

MIT License. See [LICENSE](LICENSE).
