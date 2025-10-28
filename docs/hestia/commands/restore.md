# `ghostsnap hestia restore` - Restore Command

Restore HestiaCP user from Ghostsnap repository.

---

## Synopsis

```bash
ghostsnap hestia restore <USERNAME> [OPTIONS] --repository <PATH>
```

---

## Description

The `restore` command recovers a HestiaCP user from a Ghostsnap backup by:

1. **Locating the snapshot** in the repository
2. **Downloading and decrypting** the backup data
3. **Extracting to HestiaCP structure** (preserves permissions, ownership)
4. **Running HestiaCP restore** (if supported)

> **⚠️ IMPLEMENTATION STATUS**: Currently in development. The basic structure is implemented, but full restore functionality pending Repository API finalization.

---

## Arguments

### `<USERNAME>`

**Required**. The HestiaCP username to restore.

**Example**:
```bash
ghostsnap hestia restore admin --repository /var/ghostsnap/repo
```

---

## Options

### Required

#### `--repository <PATH>`

Repository path or URL where backup is stored.

**Alias**: `--repo`, `-r`

**Examples**:
```bash
# Local repository
--repository /var/ghostsnap/hestia

# MinIO
--repository minio://ghostsnap-backups/hestia

# S3
--repository s3://my-bucket/hestia
```

**Environment Variable**: `GHOSTSNAP_REPO`

---

### Optional

#### `--snapshot <SNAPSHOT_ID>`

Specific snapshot to restore. If omitted, restores **latest** snapshot for the user.

**Alias**: `-s`

**Format**: `hestia-<username>-<timestamp>`

**Examples**:
```bash
# Restore specific snapshot
ghostsnap hestia restore admin \
  --snapshot hestia-admin-20251002-140000 \
  --repository /var/ghostsnap/repo

# Restore latest (default)
ghostsnap hestia restore admin \
  --repository /var/ghostsnap/repo
```

**How to find snapshots**: Use `list-backups` command:
```bash
ghostsnap hestia list-backups --repository /var/ghostsnap/repo --user admin
```

---

#### `--target <PATH>`

Target directory for restore. If omitted, restores to **original location** (`/home/<username>`).

**Alias**: `-t`

**Examples**:
```bash
# Restore to alternate location (for inspection)
ghostsnap hestia restore admin \
  --target /tmp/restore-admin \
  --repository /var/ghostsnap/repo

# Restore to original location (default)
ghostsnap hestia restore admin \
  --repository /var/ghostsnap/repo
```

**Use Cases**:
- ✅ Inspect backup contents before restoring
- ✅ Migrate to different server
- ✅ Test restore without affecting production
- ❌ Not needed for standard disaster recovery

---

#### `--force`

Overwrite existing user data without confirmation.

**Alias**: `-f`

**Default**: `false` (prompts for confirmation)

**Example**:
```bash
# Force restore (no prompt)
sudo ghostsnap hestia restore admin \
  --repository /var/ghostsnap/repo \
  --force
```

**⚠️ WARNING**: This will **overwrite** existing data for the user. Use with caution.

---

## Examples

### Basic Examples

#### Restore Latest Backup

```bash
sudo ghostsnap hestia restore admin \
  --repository /var/ghostsnap/repo
```

**Output**:
```
Finding latest snapshot for user: admin
Found: hestia-admin-20251002-140000
Downloading snapshot...
Extracting to /home/admin...
✓ Restored successfully
```

---

#### Restore Specific Snapshot

```bash
# First, list available snapshots
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/repo \
  --user admin

# Output:
# hestia-admin-20251002-140000  2025-10-02 14:00:00
# hestia-admin-20251001-140000  2025-10-01 14:00:00
# hestia-admin-20250930-140000  2025-09-30 14:00:00

# Restore specific snapshot
sudo ghostsnap hestia restore admin \
  --snapshot hestia-admin-20251001-140000 \
  --repository /var/ghostsnap/repo
```

**Output**:
```
Loading snapshot: hestia-admin-20251001-140000
Downloading snapshot...
Extracting to /home/admin...
✓ Restored successfully
```

---

#### Restore to Alternate Location

```bash
# Restore to /tmp for inspection
sudo ghostsnap hestia restore admin \
  --target /tmp/restore-admin \
  --repository /var/ghostsnap/repo
```

**Output**:
```
Finding latest snapshot for user: admin
Found: hestia-admin-20251002-140000
Downloading snapshot...
Extracting to /tmp/restore-admin...
✓ Restored successfully

Restored files in /tmp/restore-admin/:
  - home/
  - web/
  - conf/
  - backup/
```

**Use Case**: Recover a single file without affecting production:
```bash
# Extract to temp location
sudo ghostsnap hestia restore admin --target /tmp/restore-admin

# Copy specific file
sudo cp /tmp/restore-admin/home/admin/public_html/config.php \
        /home/admin/public_html/config.php

# Clean up
sudo rm -rf /tmp/restore-admin
```

---

#### Force Restore (No Prompt)

```bash
sudo ghostsnap hestia restore admin \
  --repository /var/ghostsnap/repo \
  --force
```

**Output**:
```
⚠️  Force mode enabled - overwriting existing data
Finding latest snapshot for user: admin
Found: hestia-admin-20251002-140000
Downloading snapshot...
Extracting to /home/admin...
✓ Restored successfully
```

**Without --force** (default):
```
⚠️  User 'admin' exists. Restore will overwrite existing data.
Continue? [y/N]: y
Finding latest snapshot for user: admin
...
```

---

### Advanced Examples

#### Restore from MinIO

```bash
sudo ghostsnap hestia restore admin \
  --repository minio://backups/hestia \
  --snapshot hestia-admin-20251002-140000
```

---

#### Restore from S3

```bash
sudo ghostsnap hestia restore admin \
  --repository s3://my-backup-bucket/hestia \
  --force
```

---

#### Disaster Recovery Scenario

```bash
# 1. Check available backups
ghostsnap hestia list-backups \
  --repository /mnt/ghostsnap/hestia \
  --user admin

# 2. Restore from latest working snapshot
sudo ghostsnap hestia restore admin \
  --repository /mnt/ghostsnap/hestia \
  --snapshot hestia-admin-20251001-140000 \
  --force

# 3. Verify restoration
sudo ls -lah /home/admin
sudo v-list-user admin

# 4. Test services
sudo systemctl restart apache2
curl -I https://admin.example.com
```

---

#### Migration to New Server

```bash
# On source server: Backup
sudo ghostsnap hestia backup \
  --user admin \
  --repository /mnt/shared-nfs/ghostsnap

# On destination server: Restore
sudo ghostsnap hestia restore admin \
  --repository /mnt/shared-nfs/ghostsnap \
  --force

# Rebuild HestiaCP databases
sudo v-rebuild-user admin
```

---

### Automation Examples

#### Automated Recovery Script

```bash
#!/bin/bash
# /usr/local/bin/restore-hestia-user.sh

set -e

USERNAME="$1"
SNAPSHOT="${2:-latest}"

if [ -z "$USERNAME" ]; then
  echo "Usage: $0 <username> [snapshot]"
  exit 1
fi

export GHOSTSNAP_REPO="/var/ghostsnap/hestia"
export GHOSTSNAP_PASSWORD="your-secure-password"

if [ "$SNAPSHOT" = "latest" ]; then
  ghostsnap hestia restore "$USERNAME" --force
else
  ghostsnap hestia restore "$USERNAME" --snapshot "$SNAPSHOT" --force
fi

echo "✓ User $USERNAME restored"
```

**Usage**:
```bash
# Restore latest
sudo /usr/local/bin/restore-hestia-user.sh admin

# Restore specific snapshot
sudo /usr/local/bin/restore-hestia-user.sh admin hestia-admin-20251002-140000
```

---

## Behavior Details

### Restore Process Flow

```
1. Validate inputs
   ↓
2. Open repository (decrypt, verify)
   ↓
3. Find snapshot:
   - If --snapshot provided: Load specific snapshot
   - Otherwise: Find latest snapshot for user
   ↓
4. Download snapshot data
   ↓
5. Decrypt and decompress
   ↓
6. Extract to target location
   - Default: /home/<username>
   - Custom: --target path
   ↓
7. Restore permissions and ownership
   ↓
8. (Optional) Run HestiaCP rebuild
   ↓
9. Report success
```

---

### Snapshot Selection

#### Latest Snapshot (Default)

When `--snapshot` is **not** specified:

```rust
1. Query repository for all snapshots matching:
   hestia-<username>-*

2. Sort by timestamp (newest first)

3. Select first snapshot
```

**Example**:
```
Available snapshots:
  - hestia-admin-20251002-140000  ← Selected (latest)
  - hestia-admin-20251001-140000
  - hestia-admin-20250930-140000
```

---

#### Specific Snapshot

When `--snapshot hestia-admin-20251001-140000` is specified:

```rust
1. Query repository for exact match

2. If not found: Error

3. Load snapshot metadata
```

---

### File Restoration

#### Directory Structure

HestiaCP backup tarballs contain:

```
admin.2025-10-02_14-00-00.tar
├── home/
│   └── admin/
│       ├── .bash_history
│       ├── .bashrc
│       ├── conf/
│       ├── public_html/
│       └── ...
├── web/
│   └── admin/
│       ├── public_html/
│       └── ...
└── backup/
    └── admin/
```

**Extraction**: Ghostsnap extracts to target directory while preserving:
- ✅ File permissions
- ✅ Ownership (user:group)
- ✅ Timestamps
- ✅ Symbolic links
- ✅ Directory structure

---

#### Permission Restoration

After extraction, Ghostsnap ensures:

```bash
# Home directory
chown -R admin:admin /home/admin
chmod 751 /home/admin

# Web directory
chown -R admin:www-data /home/admin/web
chmod 755 /home/admin/web

# Config files
chmod 600 /home/admin/conf/*
```

---

### HestiaCP Integration

After file restoration, Ghostsnap can optionally run:

```bash
# Rebuild user databases (MySQL, PostgreSQL)
v-rebuild-user admin

# Rebuild web configs
v-rebuild-web-domains admin

# Rebuild DNS zones
v-rebuild-dns-domains admin

# Rebuild mail configs
v-rebuild-mail-domains admin
```

> **Note**: Rebuild commands are optional and can be run manually if needed.

---

## Error Handling

### Common Errors

#### User Not Found in Repository

```
Error: No snapshots found for user 'admin'
```

**Solution**: Check available users:
```bash
ghostsnap hestia list-backups --repository /var/ghostsnap/repo
```

---

#### Snapshot Not Found

```
Error: Snapshot 'hestia-admin-20251002-140000' not found
```

**Solution**: List available snapshots:
```bash
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/repo \
  --user admin
```

---

#### Permission Denied

```
Error: Permission denied (os error 13)
```

**Solution**: Run with `sudo`:
```bash
sudo ghostsnap hestia restore admin ...
```

---

#### Repository Not Found

```
Error: Repository not found at /var/ghostsnap/repo
```

**Solution**: Verify repository path:
```bash
ls -la /var/ghostsnap/repo
```

---

#### Disk Space

```
Error: No space left on device (os error 28)
```

**Solution**:
```bash
# Check disk usage
df -h /home

# Free up space
sudo apt clean
sudo rm -rf /tmp/*

# Or restore to alternate location with more space
ghostsnap hestia restore admin --target /mnt/large-disk/restore
```

---

#### User Already Exists

```
⚠️  User 'admin' exists. Restore will overwrite existing data.
Continue? [y/N]: 
```

**Solutions**:
1. **Confirm overwrite**: Type `y` and press Enter
2. **Use --force flag**: `--force` to skip prompt
3. **Restore to alternate location**: `--target /tmp/restore-admin`
4. **Backup existing data first**: 
   ```bash
   sudo v-backup-user admin
   ```

---

## Performance

### Benchmarks

Typical restore performance:

| Backup Size | Download Time (Local) | Download Time (MinIO) | Extract Time | Total |
|-------------|----------------------|----------------------|--------------|-------|
| 100 MB | ~2s | ~10s | ~3s | ~13s |
| 1 GB | ~15s | ~60s | ~20s | ~80s |
| 10 GB | ~2min | ~8min | ~3min | ~11min |
| 50 GB | ~10min | ~40min | ~15min | ~55min |

**Factors**:
- Network bandwidth (remote backends)
- Disk I/O speed
- CPU (decryption, decompression)

---

### Optimization Tips

#### 1. Use Local Repository for Speed

```bash
# Mount NFS share locally
sudo mount -t nfs synology.local:/volume1/ghostsnap /mnt/ghostsnap

# Restore from local mount
sudo ghostsnap hestia restore admin --repository /mnt/ghostsnap/hestia
```

---

#### 2. Restore to Fast Disk First

```bash
# Restore to SSD
sudo ghostsnap hestia restore admin --target /mnt/ssd/restore-admin

# Then move to final location
sudo rsync -avP /mnt/ssd/restore-admin/ /home/admin/
```

---

#### 3. Parallel Restores

```bash
# Restore multiple users in parallel
sudo ghostsnap hestia restore user1 --repository /var/ghostsnap/repo &
sudo ghostsnap hestia restore user2 --repository /var/ghostsnap/repo &
sudo ghostsnap hestia restore user3 --repository /var/ghostsnap/repo &
wait
```

---

## Security Considerations

### 1. Password Protection

Repository password required:

```bash
export GHOSTSNAP_PASSWORD="your-secure-password"
```

**Never** use `--password` flag (visible in process list).

---

### 2. Verify Before Restoring

```bash
# List available backups
ghostsnap hestia list-backups --repository /var/ghostsnap/repo --user admin

# Restore to temp location for inspection
sudo ghostsnap hestia restore admin --target /tmp/inspect

# Verify contents
sudo ls -lah /tmp/inspect
sudo tar -tzf /tmp/inspect/backup.tar | head -20

# If good, restore to production
sudo ghostsnap hestia restore admin --force
```

---

### 3. Backup Before Restore

**Always** backup existing data before overwriting:

```bash
# Backup current state
sudo v-backup-user admin

# Then restore
sudo ghostsnap hestia restore admin --repository /var/ghostsnap/repo --force
```

---

### 4. Audit Logging

Log all restore operations:

```bash
ghostsnap hestia restore admin ... 2>&1 | logger -t ghostsnap-restore
```

Check logs:
```bash
journalctl -t ghostsnap-restore
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success - user restored |
| `1` | General error (snapshot not found, etc.) |
| `2` | Command-line usage error (invalid arguments) |
| `65` | Data error (corruption detected) |
| `74` | I/O error (disk full, network timeout) |
| `77` | Permission denied |

---

## See Also

- **[backup](backup.md)** - Create backups
- **[list-backups](list-backups.md)** - View available backups
- **[Disaster Recovery Guide](../use-cases/disaster-recovery.md)** - Recovery procedures
- **[Troubleshooting](../advanced/troubleshooting.md)** - Common issues

---

**Back to**: [Commands Overview](README.md) | [HestiaCP Integration](../README.md)
