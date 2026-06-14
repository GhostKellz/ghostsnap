# Snapshot Management

Snapshots are point-in-time captures of your data.

## Listing Snapshots

```bash
ghostsnap --repo /backup/repo snapshots

ID        Time                 Host      Tags         Paths
a1b2c3d4  2024-01-15 10:30:00  myhost    website      /var/www
b2c3d4e5  2024-01-14 10:30:00  myhost    website      /var/www
c3d4e5f6  2024-01-13 10:30:00  myhost                 /home
```

### Filter by Tag

```bash
ghostsnap --repo /backup/repo snapshots --tag website
```

### Filter by Host

```bash
ghostsnap --repo /backup/repo snapshots --hostname myhost
```

### Show Latest N

```bash
ghostsnap --repo /backup/repo snapshots --latest 5
```

### JSON Output

```bash
ghostsnap --repo /backup/repo snapshots --format json
```

## Browsing Snapshot Contents

### List Files

```bash
# List root of snapshot
ghostsnap --repo /backup/repo ls a1b2c3d4

# List with details (permissions, size, time)
ghostsnap --repo /backup/repo ls a1b2c3d4 -l

# List recursively
ghostsnap --repo /backup/repo ls a1b2c3d4 -r

# List specific directory
ghostsnap --repo /backup/repo ls a1b2c3d4 documents/reports
```

### Example Output

```bash
ghostsnap --repo /backup/repo ls a1b2c3d4 -l

drwxr-xr-x  chris  chris      0  2024-01-15 10:30  documents
-rw-r--r--  chris  chris   1234  2024-01-15 10:30  readme.txt
lrwxrwxrwx  chris  chris      0  2024-01-15 10:30  link -> readme.txt
```

### Extract Single File

```bash
# Dump file to stdout
ghostsnap --repo /backup/repo dump a1b2c3d4 documents/report.pdf > report.pdf

# View text file
ghostsnap --repo /backup/repo dump a1b2c3d4 config.txt | less
```

## Comparing Snapshots

```bash
# Show differences between two snapshots
ghostsnap --repo /backup/repo diff a1b2c3d4 b2c3d4e5

Added:    documents/new-file.txt
Modified: documents/report.pdf
Removed:  documents/old-file.txt

# Include metadata changes
ghostsnap --repo /backup/repo diff a1b2c3d4 b2c3d4e5 --metadata

# JSON output
ghostsnap --repo /backup/repo diff a1b2c3d4 b2c3d4e5 --json
```

## Retention Policies

### Forget Command

Mark snapshots for removal based on policies:

```bash
ghostsnap --repo /backup/repo forget \
    --keep-last 5 \
    --keep-daily 7 \
    --keep-weekly 4 \
    --keep-monthly 12 \
    --keep-yearly 3
```

### Policy Options

| Option | Description |
|--------|-------------|
| `--keep-last N` | Keep last N snapshots |
| `--keep-daily N` | Keep N daily snapshots |
| `--keep-weekly N` | Keep N weekly snapshots |
| `--keep-monthly N` | Keep N monthly snapshots |
| `--keep-yearly N` | Keep N yearly snapshots |

To prune unreferenced data immediately after forgetting, add `--prune`.

### Filter by Tag/Host

```bash
# Only apply to specific tag
ghostsnap --repo /backup/repo forget --tag website --keep-last 3

# Only apply to specific host
ghostsnap --repo /backup/repo forget --host production --keep-daily 7
```

### Dry Run

```bash
ghostsnap --repo /backup/repo forget --keep-last 5 --dry-run

Would keep:
  a1b2c3d4  2024-01-15 10:30:00  (keep-last)
  b2c3d4e5  2024-01-14 10:30:00  (keep-last)
  ...

Would remove:
  x9y8z7w6  2024-01-01 10:30:00
```

## Pruning Unused Data

After forgetting snapshots, prune removes unreferenced data:

```bash
# See what would be removed
ghostsnap --repo /backup/repo prune --dry-run

# Actually remove data
ghostsnap --repo /backup/repo prune

Removed 234 unused chunks
Freed 1.2 GB
```

## Copying Snapshots

Copy a snapshot to another repository:

```bash
# Copy snapshot to local destination
ghostsnap --repo /backup/repo copy a1b2c3d4 --repo2 /backup/offsite

# Copy to S3 destination
ghostsnap --repo /backup/repo copy a1b2c3d4 --repo2 s3:mybucket/backups

# Dry run (analyze without copying)
ghostsnap --repo /backup/repo copy a1b2c3d4 --repo2 /backup/offsite --dry-run
```

Options:
- `--repo2` - Destination repository path
- `--password2` - Password for destination repository (prompted if not provided)
- `--dry-run` - Show what would be copied without copying

## Snapshot Metadata

Each snapshot contains:
- Unique ID (UUID)
- Creation timestamp
- Hostname
- Username
- Tags
- Paths backed up
- Parent snapshot (for incrementals)
- Tree ID (root of file tree)
