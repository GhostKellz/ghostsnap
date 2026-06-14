# Repository Management

Repository operations implemented in the Ghostsnap CLI.

## Supported Repository Locations

Ghostsnap supports four repository location forms:

- **Local filesystem**: `/backup/repo`
- **S3**: `s3:my-bucket/backups`
- **Azure Blob Storage**: `azure:account/container/prefix`
- **Rclone**: `rclone:remote/path` (40+ cloud providers via rclone)

## Repository Structure

```text
repository/
├── config              # Repository configuration (JSON)
├── keys/               # Encrypted data keys
├── data/               # Pack files and tree objects
│   ├── *.pack          # Compressed, encrypted chunks
│   └── <tree-id>       # Tree metadata
├── index/              # Chunk index
│   └── main.idx        # Encrypted binary index
├── snapshots/          # Snapshot metadata
│   └── <snapshot-id>   # Encrypted snapshot data
└── locks/              # Repository locks
```

## Initializing

### Local Repository

```bash
ghostsnap init /backup/repo
```

### S3 Repository

```bash
# Using flags (constructs URI automatically)
ghostsnap init --backend s3 --bucket my-bucket --prefix backups

# With explicit URI
ghostsnap init --backend s3 --bucket my-bucket --prefix backups s3:my-bucket/backups

# S3-compatible storage (MinIO, Wasabi, Backblaze B2)
ghostsnap init --backend s3 --bucket backups --endpoint http://localhost:9000
```

### Azure Repository

```bash
# Using flags (constructs URI automatically)
ghostsnap init --backend azure --account-name mystorageaccount --container backups

# With prefix
ghostsnap init --backend azure --account-name mystorageaccount --container backups --azure-prefix production
```

### Rclone Repository

Requires rclone to be installed and configured (`rclone config`).

```bash
# Using flags (constructs URI automatically)
ghostsnap init --backend rclone --remote myremote --rclone-path backups/ghostsnap

# With explicit URI
ghostsnap init --repo rclone:myremote/backups/ghostsnap

# Examples with common providers
ghostsnap init --backend rclone --remote gdrive --rclone-path ghostsnap    # Google Drive
ghostsnap init --backend rclone --remote dropbox --rclone-path backups     # Dropbox
ghostsnap init --backend rclone --remote sftp-server --rclone-path /backups # SFTP
```

## Checking Integrity

```bash
# Local repository
ghostsnap --repo /backup/repo check

# S3 repository
ghostsnap --repo s3:my-bucket/backups check

# Azure repository
ghostsnap --repo azure:mystorageaccount/backups check

# Rclone repository
ghostsnap --repo rclone:myremote/backups check

# Full check (reads pack data)
ghostsnap --repo /backup/repo check --read-data
```

## Repository Statistics

```bash
ghostsnap --repo /backup/repo stats
ghostsnap --repo s3:my-bucket/backups stats
ghostsnap --repo azure:mystorageaccount/backups stats
ghostsnap --repo rclone:myremote/backups stats
```

## Snapshot Retention

```bash
# Keep only the last 7 snapshots
ghostsnap --repo /backup/repo forget --keep-last 7

# Same workflow against S3
ghostsnap --repo s3:my-bucket/backups forget --keep-last 7

# Then reclaim unused data
ghostsnap --repo /backup/repo prune
```

## Comparing Snapshots

```bash
ghostsnap --repo /backup/repo diff <snapshot-a> <snapshot-b>
ghostsnap --repo s3:my-bucket/backups diff <snapshot-a> <snapshot-b>
```

## Listing Files

```bash
ghostsnap --repo /backup/repo ls <snapshot-id>
ghostsnap --repo /backup/repo ls <snapshot-id> -r
ghostsnap --repo s3:my-bucket/backups ls <snapshot-id>
```

## Dumping a File

```bash
ghostsnap --repo /backup/repo dump <snapshot-id> path/to/file > restored-file
ghostsnap --repo s3:my-bucket/backups dump <snapshot-id> path/to/file > restored-file
```

## Copying Snapshots

Copy snapshots between repositories. Requires a snapshot ID.

```bash
# Local to local
ghostsnap --repo /backup/source copy --repo2 /backup/destination <snapshot-id>

# Local to S3
ghostsnap --repo /backup/source copy --repo2 s3:my-bucket/backups <snapshot-id>

# Local to Azure
ghostsnap --repo /backup/source copy --repo2 azure:mystorageaccount/backups <snapshot-id>

# Local to rclone (any provider)
ghostsnap --repo /backup/source copy --repo2 rclone:gdrive/backups <snapshot-id>

# S3 to local
ghostsnap --repo s3:my-bucket/backups copy --repo2 /backup/local <snapshot-id>

# Azure to local
ghostsnap --repo azure:mystorageaccount/backups copy --repo2 /backup/local <snapshot-id>

# Rclone to local
ghostsnap --repo rclone:gdrive/backups copy --repo2 /backup/local <snapshot-id>
```

## Repository Locking

Ghostsnap uses repository locking to prevent concurrent operations from corrupting repository data.

### Current Behavior

**Local repositories:** Full locking support. A lock file is created in the `locks/` directory before any write operation (backup, forget, prune). The lock is released when the operation completes.

**Remote repositories (S3, Azure, Rclone):** Local locking only. Ghostsnap prevents concurrent operations from the *same machine*, but does not coordinate locks across multiple machines.

### Single-Writer Recommendation

For remote repositories, follow a single-writer pattern:

1. **Run backups from one machine** - Designate a single host to back up to each remote repository
2. **Avoid overlapping jobs** - Schedule backup jobs to run at different times
3. **Use job configs** - The `ghostsnap job` command runs backup, forget, and prune sequentially, preventing overlap

### Safe Patterns

**Sequential job execution (recommended):**
```bash
# One job config runs all operations sequentially
ghostsnap job --config /etc/ghostsnap/jobs.toml run website-b2
```

**Staggered scheduling:**
```bash
# Different jobs at different times
# 02:00 - website-b2
# 03:00 - database-b2
# 04:00 - docker-b2
```

**Dedicated backup host:**
```bash
# Pull data to a backup server, then push to remote storage
rsync -a web-01:/var/www /staging/web-01/
ghostsnap --repo s3:my-bucket/web-01 backup /staging/web-01
```

### Unsafe Patterns

Avoid these patterns with remote repositories:

```bash
# DON'T: Concurrent backups from multiple hosts
# Host A runs: ghostsnap --repo s3:bucket/shared backup /data
# Host B runs: ghostsnap --repo s3:bucket/shared backup /other  # Concurrent - unsafe
```

```bash
# DON'T: Overlapping jobs to the same repository
# Cron: 02:00 ghostsnap backup ...
# Cron: 02:05 ghostsnap prune ...  # May overlap - unsafe
```

### Detecting Conflicts

If you suspect repository corruption from concurrent access:

```bash
# Check repository integrity
ghostsnap --repo s3:my-bucket/backups check

# Full verification (reads all data)
ghostsnap --repo s3:my-bucket/backups check --read-data
```

### Future Roadmap

Remote locking is planned for a future release:
- S3: Object-based locking with conditional writes
- Azure: Blob lease-based locking
- Rclone: File-based locking with TTL

Until then, use single-writer patterns to ensure repository integrity.
