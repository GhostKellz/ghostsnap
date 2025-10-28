# `ghostsnap hestia backup` - Backup Command

Backup HestiaCP user(s) to Ghostsnap repository.

---

## Synopsis

```bash
ghostsnap hestia backup [OPTIONS] --repository <PATH>
```

---

## Description

The `backup` command creates backups of HestiaCP users by:

1. **Invoking HestiaCP's native backup** (`v-backup-user`)
2. **Locating the generated tarball** in `/backup/`
3. **Uploading to Ghostsnap repository** (MinIO, S3, Azure, Local)
4. **Creating snapshot metadata** with timestamp and username
5. **Cleaning up old tarballs** (optional)

This is a **wrapper approach** - it leverages HestiaCP's proven backup logic while adding:
- ✅ Deduplication
- ✅ Encryption
- ✅ Compression
- ✅ Multiple backend support
- ✅ Automatic cleanup

---

## Options

### Required

#### `--repository <PATH>`

Repository path or URL where backups will be stored.

**Alias**: `--repo`, `-r`

**Examples**:
```bash
# Local repository
--repository /var/ghostsnap/hestia

# MinIO
--repository minio://ghostsnap-backups/hestia

# S3
--repository s3://my-bucket/hestia

# Azure
--repository azure://container/hestia
```

**Environment Variable**: `GHOSTSNAP_REPO`
```bash
export GHOSTSNAP_REPO="/var/ghostsnap/hestia"
ghostsnap hestia backup  # No --repository needed
```

---

### User Selection

Choose which user(s) to backup:

#### `--user <USERNAME>`

Backup a **single user**.

**Alias**: `-u`

**Example**:
```bash
ghostsnap hestia backup --user admin --repository /var/ghostsnap/repo
```

**Validation**: Command fails if user doesn't exist.

---

#### No user flag (default)

Backup **all HestiaCP users**.

**Example**:
```bash
ghostsnap hestia backup --repository /var/ghostsnap/repo
```

**Discovery**: Automatically finds all users in `/usr/local/hestia/data/users/`.

---

### Pattern Matching

Filter users with glob patterns:

#### `--include <PATTERN>`

Include users matching pattern. Supports `*` wildcard.

**Examples**:
```bash
# Backup all production users
--include "prod-*"

# Backup specific pattern
--include "client-*-live"

# Multiple includes (first match wins)
--include "app-*" --include "site-*"
```

**Behavior**: 
- Only users matching pattern are backed up
- Patterns are checked with glob matching: `*` matches any characters
- Cannot combine with `--exclude`

---

#### `--exclude <PATTERN>`

Exclude users matching pattern. Supports `*` wildcard.

**Examples**:
```bash
# Skip test users
--exclude "test-*"

# Skip dev environments
--exclude "*-dev"
--exclude "*-staging"

# Multiple excludes
--exclude "temp-*" --exclude "old-*"
```

**Behavior**:
- Users matching pattern are skipped
- Cannot combine with `--include`

---

### Cleanup

Manage HestiaCP backup tarballs in `/backup/`:

#### `--cleanup`

Enable automatic cleanup of old tarballs.

**Default**: `false` (tarballs accumulate indefinitely)

**Example**:
```bash
ghostsnap hestia backup --repository /var/ghostsnap/repo --cleanup
```

**When to use**:
- ✅ Production systems (prevent disk fill)
- ✅ Automated daily backups
- ❌ Manual backups (may want to verify before deleting)

---

#### `--keep-tarballs <N>`

Number of tarballs to keep per user.

**Default**: `3`

**Range**: `0` to `999`

**Examples**:
```bash
# Keep only 1 (most recent)
--cleanup --keep-tarballs 1

# Keep 7 days of backups
--cleanup --keep-tarballs 7

# Delete immediately after upload
--cleanup --keep-tarballs 0
```

**Behavior**:
- Only has effect when `--cleanup` is enabled
- Keeps the **N most recent** tarballs by modification time
- Deletes older tarballs
- Per-user basis (each user keeps N tarballs)

---

## Examples

### Basic Examples

#### Backup Single User

```bash
sudo ghostsnap hestia backup \
  --user admin \
  --repository /var/ghostsnap/repo
```

**Output**:
```
Backing up user: admin
Running: v-backup-user admin
Finding tarball for admin
Found: /backup/admin.2025-10-02_14-00-00.tar
Uploading to repository...
✓ Snapshot created: hestia-admin-20251002-140000
```

---

#### Backup All Users

```bash
sudo ghostsnap hestia backup \
  --repository /var/ghostsnap/repo
```

**Output**:
```
Found 5 users: admin, alice, bob, charlie, dev-tester
Backing up user: admin
✓ Snapshot created: hestia-admin-20251002-140000
Backing up user: alice
✓ Snapshot created: hestia-alice-20251002-140000
...
✓ Backed up 5 users successfully
```

---

#### Backup with Cleanup

```bash
sudo ghostsnap hestia backup \
  --repository /var/ghostsnap/repo \
  --cleanup \
  --keep-tarballs 3
```

**Output**:
```
Backing up user: admin
✓ Snapshot created: hestia-admin-20251002-140000
Cleaning up old tarballs for admin...
Keeping 3 most recent:
  - admin.2025-10-02_14-00-00.tar
  - admin.2025-10-01_14-00-00.tar
  - admin.2025-09-30_14-00-00.tar
Deleting 2 old tarballs:
  - admin.2025-09-29_14-00-00.tar
  - admin.2025-09-28_14-00-00.tar
```

---

### Pattern Matching Examples

#### Include Pattern

```bash
# Backup only production users
sudo ghostsnap hestia backup \
  --repository /var/ghostsnap/repo \
  --include "prod-*" \
  --cleanup
```

**Scenario**: Users are `prod-web`, `prod-api`, `dev-test`, `staging-app`

**Output**:
```
Found 4 users
Filtered by pattern "prod-*": 2 users
Backing up user: prod-web
✓ Snapshot created: hestia-prod-web-20251002-140000
Backing up user: prod-api
✓ Snapshot created: hestia-prod-api-20251002-140000
```

---

#### Exclude Pattern

```bash
# Backup all except test users
sudo ghostsnap hestia backup \
  --repository /var/ghostsnap/repo \
  --exclude "test-*" \
  --exclude "*-dev" \
  --cleanup
```

**Scenario**: Users are `admin`, `prod-web`, `test-user`, `staging-dev`

**Output**:
```
Found 4 users
Excluded by pattern: test-user, staging-dev
Backing up user: admin
✓ Snapshot created: hestia-admin-20251002-140000
Backing up user: prod-web
✓ Snapshot created: hestia-prod-web-20251002-140000
```

---

### Advanced Examples

#### MinIO Backend

```bash
# First time: Initialize repository
ghostsnap init minio://backups/hestia \
  --minio-endpoint https://minio.example.com \
  --minio-access-key AKIAIOSFODNN7EXAMPLE \
  --minio-secret-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

# Backup
sudo ghostsnap hestia backup \
  --repository minio://backups/hestia \
  --cleanup \
  --keep-tarballs 7
```

---

#### Local Storage (Synology NFS)

```bash
# Mount NFS share
sudo mount -t nfs synology.local:/volume1/ghostsnap /mnt/ghostsnap

# Initialize repository
ghostsnap init /mnt/ghostsnap/hestia

# Backup
sudo ghostsnap hestia backup \
  --repository /mnt/ghostsnap/hestia \
  --cleanup
```

---

#### S3 Backend

```bash
# Initialize with S3
ghostsnap init s3://my-backup-bucket/hestia

# Backup
sudo ghostsnap hestia backup \
  --repository s3://my-backup-bucket/hestia \
  --exclude "temp-*" \
  --cleanup \
  --keep-tarballs 14  # 2 weeks
```

---

### Automation Examples

#### Daily Cron Job

```bash
#!/bin/bash
# /etc/cron.daily/ghostsnap-backup

export GHOSTSNAP_REPO="/var/ghostsnap/hestia"
export GHOSTSNAP_PASSWORD="your-secure-password"

/usr/local/bin/ghostsnap hestia backup \
  --exclude "test-*" \
  --cleanup \
  --keep-tarballs 7 \
  2>&1 | logger -t ghostsnap
```

**Install**:
```bash
sudo chmod +x /etc/cron.daily/ghostsnap-backup
```

---

#### Systemd Timer

**Service** (`/etc/systemd/system/ghostsnap-backup.service`):
```ini
[Unit]
Description=Ghostsnap HestiaCP Backup
After=network.target

[Service]
Type=oneshot
Environment="GHOSTSNAP_REPO=/var/ghostsnap/hestia"
Environment="GHOSTSNAP_PASSWORD=your-secure-password"
ExecStart=/usr/local/bin/ghostsnap hestia backup \
  --exclude "test-*" \
  --cleanup \
  --keep-tarballs 7
StandardOutput=journal
StandardError=journal
```

**Timer** (`/etc/systemd/system/ghostsnap-backup.timer`):
```ini
[Unit]
Description=Daily Ghostsnap Backup

[Timer]
OnCalendar=daily
OnCalendar=02:00
Persistent=true

[Install]
WantedBy=timers.target
```

**Enable**:
```bash
sudo systemctl daemon-reload
sudo systemctl enable ghostsnap-backup.timer
sudo systemctl start ghostsnap-backup.timer
```

---

## Behavior Details

### Backup Process Flow

```
1. Validate inputs
   ↓
2. Discover users (or validate single user)
   ↓
3. Apply pattern filters (include/exclude)
   ↓
4. For each user:
   a. Run v-backup-user <username>
   b. Find latest tarball in /backup/
   c. Upload to repository (dedupe, encrypt, compress)
   d. Create snapshot metadata
   e. Cleanup old tarballs (if --cleanup)
   ↓
5. Report summary
```

---

### Tarball Discovery

After `v-backup-user` runs, Ghostsnap searches for the tarball:

**Search Pattern**:
```
/backup/<username>.<TIMESTAMP>.tar
```

**Example**:
```
/backup/admin.2025-10-02_14-00-00.tar
```

**Fallback**: If timestamped tarball not found, searches for:
```
/backup/<username>.tar
```

---

### Snapshot Naming

Snapshots are named using the pattern:
```
hestia-<username>-<TIMESTAMP>
```

**Examples**:
```
hestia-admin-20251002-140000
hestia-alice-20251002-140530
hestia-prod-web-20251002-141200
```

**Timestamp Format**: `YYYYMMdd-HHmmss` (UTC)

---

### Cleanup Algorithm

When `--cleanup --keep-tarballs N` is specified:

```rust
1. List all tarballs for user in /backup/
   Pattern: /backup/<username>.*.tar

2. Sort by modification time (newest first)

3. Keep first N tarballs

4. Delete remaining tarballs
```

**Example**:
```
User: admin
Tarballs found:
  - admin.2025-10-02_14-00-00.tar (newest)
  - admin.2025-10-01_14-00-00.tar
  - admin.2025-09-30_14-00-00.tar
  - admin.2025-09-29_14-00-00.tar
  - admin.2025-09-28_14-00-00.tar (oldest)

With --keep-tarballs 3:
  Keep: 3 newest
  Delete: 2 oldest
```

---

## Error Handling

### Common Errors

#### User Not Found

```
Error: User 'notexist' not found
```

**Solution**: Check username spelling, use `list-users` to see available users.

---

#### Permission Denied

```
Error: Permission denied (os error 13)
```

**Solution**: Run with `sudo`:
```bash
sudo ghostsnap hestia backup ...
```

---

#### Repository Not Found

```
Error: Repository not found at /var/ghostsnap/repo
```

**Solution**: Initialize repository first:
```bash
ghostsnap init /var/ghostsnap/repo
```

---

#### Tarball Not Found

```
Error: Could not find backup tarball for user 'admin' in /backup/
```

**Causes**:
- HestiaCP backup failed
- Tarball already deleted
- Insufficient permissions

**Solution**:
```bash
# Check HestiaCP backup manually
sudo v-backup-user admin

# Verify tarball exists
ls -lh /backup/admin.*
```

---

#### Disk Space

```
Error: No space left on device (os error 28)
```

**Solution**:
```bash
# Check disk usage
df -h /backup

# Enable aggressive cleanup
ghostsnap hestia backup --cleanup --keep-tarballs 1

# Or clean manually
sudo rm /backup/*.tar
```

---

## Performance

### Benchmarks

Typical performance on modern hardware:

| User Size | Backup Time | Upload Time (Local) | Upload Time (MinIO) |
|-----------|-------------|---------------------|---------------------|
| 100 MB | ~5s | ~2s | ~10s |
| 1 GB | ~30s | ~15s | ~60s |
| 10 GB | ~5min | ~2min | ~8min |
| 50 GB | ~20min | ~10min | ~40min |

**Factors**:
- Disk I/O speed
- Network bandwidth (remote backends)
- CPU (compression, encryption)
- Deduplication ratio

---

### Optimization Tips

#### 1. Use Local Repository for Speed

```bash
# Fastest: Local repository
ghostsnap hestia backup --repository /var/ghostsnap/repo
```

#### 2. Schedule During Off-Peak Hours

```bash
# Cron: 2 AM daily
0 2 * * * /usr/local/bin/ghostsnap hestia backup ...
```

#### 3. Enable Cleanup to Save Disk

```bash
# Keep only 3 tarballs
ghostsnap hestia backup --cleanup --keep-tarballs 3
```

#### 4. Use Pattern Matching for Large Servers

```bash
# Backup production users only
ghostsnap hestia backup --include "prod-*"
```

---

## Security Considerations

### 1. Password Protection

**Always** set a repository password:

```bash
export GHOSTSNAP_PASSWORD="your-secure-password"
```

**Never** use `--password` flag in scripts (visible in process list).

---

### 2. Tarball Cleanup

Tarballs in `/backup/` are **unencrypted**. Enable cleanup:

```bash
ghostsnap hestia backup --cleanup --keep-tarballs 0
```

---

### 3. Backend Security

#### MinIO/S3
- Use HTTPS endpoints
- Rotate access keys regularly
- Enable bucket versioning
- Use bucket policies (principle of least privilege)

#### Local Storage
- Restrict permissions: `chmod 700 /var/ghostsnap`
- Use encrypted filesystem (LUKS)
- Mount NFS with `sec=krb5` (Kerberos)

---

### 4. Audit Logging

Log all backup operations:

```bash
ghostsnap hestia backup ... 2>&1 | logger -t ghostsnap
```

Check logs:
```bash
journalctl -t ghostsnap
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success - all users backed up |
| `1` | General error (user not found, permission denied, etc.) |
| `2` | Command-line usage error (invalid arguments) |
| `65` | Data error (tarball not found, corruption detected) |
| `74` | I/O error (disk full, network timeout) |
| `77` | Permission denied |

---

## See Also

- **[restore](restore.md)** - Restore from backup
- **[list-backups](list-backups.md)** - View available backups
- **[Backup Strategies](../use-cases/backup-strategies.md)** - Best practices
- **[Automation Guide](../use-cases/automation.md)** - Systemd and cron setup

---

**Back to**: [Commands Overview](README.md) | [HestiaCP Integration](../README.md)
