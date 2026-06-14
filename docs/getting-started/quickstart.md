# Quick Start Guide

This guide walks you through creating your first backup with Ghostsnap.

## 1. Initialize a Repository

Create a new encrypted repository:

```bash
# Local repository
ghostsnap init /backup/ghostsnap
Enter repository password: ********
Repository initialized at /backup/ghostsnap

# S3 repository
ghostsnap init --backend s3 --bucket my-bucket --prefix backups
```

## 2. Create a Backup

Back up a directory:

```bash
ghostsnap --repo /backup/ghostsnap backup /home/user/documents
Enter repository password: ********

Scanning files...
Found 1,234 files, 56 dirs, 12 symlinks (256.5 MB)
Backing up 1,302 items...
[####################] 256.5 MB/256.5 MB (45.2 MB/s, ETA: 0s)
Done (892 new, 340 dedup, 256.5 MB @ 45.2 MB/s)

Backup completed successfully!
Snapshot: a1b2c3d4
Files: 1234 | Dirs: 56 | Symlinks: 12
Size: 256.5 MB | New chunks: 892 | Dedup chunks: 340
Tree: e5f6g7h8

# Same command shape against S3
ghostsnap --repo s3:my-bucket/backups backup /home/user/documents
```

## 3. List Snapshots

View all snapshots in the repository:

```bash
ghostsnap --repo /backup/ghostsnap snapshots

ID        Time                 Host      Tags    Paths
a1b2c3d4  2024-01-15 10:30:00  myhost            /home/user/documents
b2c3d4e5  2024-01-14 10:30:00  myhost            /home/user/documents
```

## 4. Browse a Snapshot

List files in a snapshot:

```bash
# List root of snapshot
ghostsnap --repo /backup/ghostsnap ls a1b2c3d4

# List with details
ghostsnap --repo /backup/ghostsnap ls a1b2c3d4 -l

# List recursively
ghostsnap --repo /backup/ghostsnap ls a1b2c3d4 -r
```

## 5. Restore Files

Restore from a snapshot:

```bash
# Restore entire snapshot
ghostsnap --repo /backup/ghostsnap restore a1b2c3d4 --target /restore/dir

# Restore specific paths
ghostsnap --repo /backup/ghostsnap restore a1b2c3d4 --target /restore/dir documents/important

# Dry run first
ghostsnap --repo /backup/ghostsnap restore a1b2c3d4 --target /restore/dir --dry-run
```

## 6. Verify Repository

Check repository integrity:

```bash
# Quick check (metadata only)
ghostsnap --repo /backup/ghostsnap check

# Full check (read all data)
ghostsnap --repo /backup/ghostsnap check --read-data
```

## Common Options

| Option | Description |
|--------|-------------|
| `--repo` | Repository location (global, before subcommand) |
| `--password` | Repository password (or use GHOSTSNAP_PASSWORD) |
| `--verbose`, `-v` | Verbose output |
| `--quiet`, `-q` | Quiet mode |

## Environment Variables

```bash
export GHOSTSNAP_REPO=/backup/ghostsnap
export GHOSTSNAP_PASSWORD=secret

# Now commands don't need --repo or password prompt
ghostsnap backup /home/user/documents
ghostsnap snapshots
```

## Next Steps

- [Backup Guide](../usage/backup.md) - Advanced backup options
- [Restore Guide](../usage/restore.md) - Restore options and verification
- [Backend Setup](../backends/s3.md) - S3 and cloud storage configuration
