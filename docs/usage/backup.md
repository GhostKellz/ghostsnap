# Backup Command

The `backup` command creates a snapshot of files and directories.

## Basic Usage

```bash
ghostsnap --repo /backup/repo backup /path/to/backup
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--tag` | | Add tags to snapshot |
| `--exclude` | `-e` | Exclude patterns (glob) |
| `--exclude-if-present` | | Skip directories containing this file |
| `--one-file-system` | `-x` | Stay on same filesystem |
| `--dry-run` | `-n` | Show what would be backed up |
| `--parent` | | Parent snapshot for incremental |
| `--hostname` | | Override hostname |
| `--no-xattr` | | Don't backup extended attributes |
| `--no-hardlinks` | | Don't detect/preserve hardlinks |
| `--max-file-size` | | Skip files larger than this |

Note: `--repo` is a global option specified before the subcommand.

## Examples

### Multiple Paths

```bash
ghostsnap --repo /backup/repo backup /home /etc /var/www
```

### With Tags

```bash
ghostsnap --repo /backup/repo backup /var/www --tag=website --tag=production
```

### Exclude Patterns

```bash
ghostsnap --repo /backup/repo backup /home \
    -e "*.log" \
    -e "*.tmp" \
    -e ".cache" \
    -e "node_modules"
```

### Skip Large Files

```bash
# Skip files larger than 1GB
ghostsnap --repo /backup/repo backup /data --max-file-size 1G
```

### Incremental Backup

Use a parent snapshot to speed up scanning:

```bash
ghostsnap --repo /backup/repo backup /data --parent a1b2c3d4
```

### Exclude Directories with Marker

Skip directories containing `.nobackup`:

```bash
ghostsnap --repo /backup/repo backup /data --exclude-if-present .nobackup
```

### Dry Run

See what would be backed up without creating a snapshot:

```bash
ghostsnap --repo /backup/repo backup /data --dry-run

DRY RUN - no data will be written
Found 1,234 files, 56 dirs, 12 symlinks (256.5 MB)
Dry run completed - would backup 1,234 files, 56 dirs, 12 symlinks (256.5 MB)
```

## Progress Output

During backup, you'll see:

```
Scanning files...
Found 1,234 files, 56 dirs, 12 symlinks (256.5 MB)
Backing up 1,302 items...
[####################] 256.5 MB/256.5 MB (45.2 MB/s, ETA: 0s) Processing file.txt
Done (892 new, 340 dedup, 256.5 MB @ 45.2 MB/s)

Backup completed successfully!
Snapshot: a1b2c3d4
Files: 1234 | Dirs: 56 | Symlinks: 12
Size: 256.5 MB | New chunks: 892 | Dedup chunks: 340
Time: 5s @ 45.2 MB/s
Tree: e5f6g7h8
```

## What Gets Backed Up

For each file, Ghostsnap stores:
- File contents (deduplicated, compressed, encrypted)
- Permissions (mode)
- Owner/group (uid/gid)
- Modification time (mtime)
- Symlink targets
- Extended attributes (xattr)
- Sparse file holes
- Hardlink relationships

## Deduplication

Ghostsnap uses content-defined chunking (FastCDC) to deduplicate data:
- Files are split into variable-size chunks (avg 4MB)
- Chunks are identified by BLAKE3 hash
- Identical chunks are stored only once
- Deduplication works across all backups

## Hardlinks

Files with the same inode (hardlinks) are detected and preserved:
- Only one copy of the data is stored
- Hardlink relationships are recorded in metadata
- Restored correctly with `--no-hardlinks` to create copies instead

## Performance Tips

1. **Use exclude patterns** to skip unnecessary files
2. **Use `--max-file-size`** to skip very large files
3. **Use incremental `--parent`** for faster scanning
4. **Run from local network** to cloud storage when possible
