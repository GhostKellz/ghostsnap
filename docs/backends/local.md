# Local Storage Backend

The local backend stores data on the local filesystem.

## Usage

```bash
# Initialize
ghostsnap init /path/to/repo

# Backup
ghostsnap --repo /path/to/repo backup /data
```

## Path Formats

```bash
# Absolute path
ghostsnap init /backup/ghostsnap

# Relative path (from current directory)
ghostsnap init ./backups

# Home directory
ghostsnap init ~/backups
```

## Permissions

The repository directory needs:
- Read/write access for the user running ghostsnap
- Execute permission on directories

```bash
# Create with appropriate permissions
mkdir -p /backup/ghostsnap
chmod 700 /backup/ghostsnap
```

## Performance

For best performance with local storage:
- Use SSD for the repository
- Ensure adequate free space (at least 2x expected backup size)
- Consider using a dedicated partition

## Network Filesystems

Local backend works with network mounts:

```bash
# NFS mount
mount -t nfs server:/backup /mnt/backup
ghostsnap init --repo /mnt/backup/ghostsnap

# CIFS/SMB mount
mount -t cifs //server/backup /mnt/backup -o username=user
ghostsnap init --repo /mnt/backup/ghostsnap
```

Note: Network filesystems may be slower than cloud backends due to protocol overhead.

## Recommended Directory Structure

```
/backup/
└── ghostsnap/
    ├── server1/          # Per-host repositories
    ├── server2/
    └── workstation/
```
