# Restore Command

The `restore` command extracts files from a snapshot.

## Basic Usage

```bash
ghostsnap --repo /backup/repo restore SNAPSHOT_ID --target /restore/path
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--repo` | `-r` | Repository location (global, before subcommand) |
| `--target` | `-t` | Target directory for restore |
| `--no-permissions` | | Don't restore file permissions |
| `--no-ownership` | | Don't restore uid/gid (requires root) |
| `--overwrite` | | Overwrite existing files |
| `--dry-run` | `-n` | Show what would be restored |
| `--no-xattr` | | Don't restore extended attributes |
| `--sparse` | | Restore sparse files with holes |
| `--verify` | | Verify restored files by hash |
| `--no-hardlinks` | | Create copies instead of hardlinks |

## Examples

### Restore Entire Snapshot

```bash
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore
```

### Restore Specific Paths

```bash
# Restore only the documents directory
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore documents

# Restore multiple paths
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore documents photos
```

### Using Short Snapshot IDs

You can use short prefixes:

```bash
ghostsnap --repo /backup/repo restore a1b2 --target /restore
```

### Dry Run

See what would be restored:

```bash
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore --dry-run

Would create directory: /restore/documents
Would restore file: /restore/documents/report.pdf (1.2 MB)
Would create symlink: /restore/documents/latest -> report.pdf
```

### Overwrite Existing

By default, existing files are skipped:

```bash
# Skip existing (default)
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore
Skipped (existing): 45

# Overwrite existing
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore --overwrite
```

### Verify After Restore

Verify file integrity by recomputing hashes:

```bash
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore --verify

Restore completed!
Restored: 1234 (256.5 MB in 12s)
Verified: 1234 | Failed: 0
Location: /restore
```

### Restore Sparse Files

Restore files with holes efficiently:

```bash
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore --sparse
```

### Without Permissions

Useful when restoring as non-root:

```bash
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore \
    --no-permissions --no-ownership
```

## Progress Output

```
Restoring snapshot: a1b2c3d4
Created: 2024-01-15 10:30:00 UTC
Host: myhost
User: chris
Target: /restore

Restoring 56 dirs, 1234 files, 12 symlinks...
[####################] 256.5 MB/256.5 MB (89.2 MB/s, ETA: 0s) Restoring file.txt
Done (256.5 MB @ 89.2 MB/s)

Restore completed!
Restored: 1302 (256.5 MB in 2s)
Location: /restore
```

## What Gets Restored

- File contents (decompressed, decrypted)
- Directory structure
- File permissions (mode)
- Owner/group (uid/gid) - requires root
- Modification time (mtime)
- Symlinks with correct targets
- Extended attributes (xattr)
- Sparse file holes (with `--sparse`)
- Hardlinks (or copies with `--no-hardlinks`)

## Restoring to Different Location

You can restore anywhere:

```bash
# Original backup path: /home/user/documents
# Restore to different location:
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /tmp/recovery
# Files appear at /tmp/recovery/documents/...
```

## Partial Restore

Restore specific directories or files:

```bash
# List contents first
ghostsnap --repo /backup/repo ls a1b2c3d4

# Restore only what you need
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore important-dir

# Restore files matching pattern (use ls to find paths first)
ghostsnap --repo /backup/repo ls a1b2c3d4 -r | grep "\.pdf$"
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore documents/reports
```

## Troubleshooting

### Permission Denied

Run as root or use `--no-ownership`:

```bash
sudo ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore
# or
ghostsnap --repo /backup/repo restore a1b2c3d4 --target /restore --no-ownership
```

### Disk Space

Check available space before restoring:

```bash
ghostsnap --repo /backup/repo ls a1b2c3d4 -l | tail -1
# Shows total size
```

### Verification Failed

If `--verify` reports failures:

```bash
# Check repository integrity
ghostsnap --repo /backup/repo check --read-data

# Try restoring individual files
ghostsnap --repo /backup/repo dump a1b2c3d4 path/to/file > /tmp/test
```
