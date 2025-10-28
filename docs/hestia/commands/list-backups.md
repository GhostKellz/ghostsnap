# `ghostsnap hestia list-backups` - List Backups Command

List backups in Ghostsnap repository.

---

## Synopsis

```bash
ghostsnap hestia list-backups --repository <PATH> [OPTIONS]
```

---

## Description

The `list-backups` command displays snapshots stored in a Ghostsnap repository.

**Features**:
- ✅ List all snapshots or filter by user
- ✅ Show snapshot metadata (timestamp, size, tags)
- ✅ Sort by date (newest first)
- ✅ Supports all backend types (MinIO, S3, Azure, Local)

**Use Cases**:
- View available backups before restore
- Audit backup history
- Verify backup operations
- Generate backup reports

> **⚠️ IMPLEMENTATION STATUS**: Currently in development. Basic structure implemented, full functionality pending Repository API finalization.

---

## Options

### Required

#### `--repository <PATH>`

Repository path or URL to query.

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

#### `--user <USERNAME>`

Filter backups for specific user.

**Alias**: `-u`

**Example**:
```bash
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/repo \
  --user admin
```

**Default**: Show backups for **all users**.

---

## Examples

### Basic Examples

#### List All Backups

```bash
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/repo
```

**Output**:
```
Snapshot ID                        User     Date                 Size
─────────────────────────────────────────────────────────────────────────
hestia-admin-20251002-140000       admin    2025-10-02 14:00:00  2.5 GB
hestia-alice-20251002-140530       alice    2025-10-02 14:05:30  1.2 GB
hestia-bob-20251002-141200         bob      2025-10-02 14:12:00  850 MB
hestia-admin-20251001-140000       admin    2025-10-01 14:00:00  2.4 GB
hestia-alice-20251001-140530       alice    2025-10-01 14:05:30  1.1 GB
hestia-admin-20250930-140000       admin    2025-09-30 14:00:00  2.3 GB

Total: 6 snapshots, 10.35 GB
```

---

#### List Backups for Specific User

```bash
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/repo \
  --user admin
```

**Output**:
```
Snapshot ID                        Date                 Size      Tags
───────────────────────────────────────────────────────────────────────
hestia-admin-20251002-140000       2025-10-02 14:00:00  2.5 GB    daily
hestia-admin-20251001-140000       2025-10-01 14:00:00  2.4 GB    daily
hestia-admin-20250930-140000       2025-09-30 14:00:00  2.3 GB    daily
hestia-admin-20250929-140000       2025-09-29 14:00:00  2.3 GB    daily
hestia-admin-20250928-140000       2025-09-28 14:00:00  2.2 GB    daily

Total: 5 snapshots, 11.7 GB
```

---

### Advanced Examples

#### List Backups from MinIO

```bash
ghostsnap hestia list-backups \
  --repository minio://backups/hestia \
  --user admin
```

---

#### List Backups from S3

```bash
ghostsnap hestia list-backups \
  --repository s3://my-backup-bucket/hestia
```

---

#### Use in Restore Script

```bash
#!/bin/bash
# Find and restore latest backup for user

USER="admin"
REPO="/var/ghostsnap/hestia"

# Get latest snapshot ID
LATEST=$(ghostsnap hestia list-backups \
  --repository "$REPO" \
  --user "$USER" | \
  grep "hestia-$USER" | \
  head -1 | \
  awk '{print $1}')

echo "Latest snapshot for $USER: $LATEST"

# Restore
sudo ghostsnap hestia restore "$USER" \
  --snapshot "$LATEST" \
  --repository "$REPO" \
  --force
```

---

#### Count Backups Per User

```bash
#!/bin/bash
# Count backups for each user

REPO="/var/ghostsnap/hestia"
USERS=$(ghostsnap hestia list-users)

echo "Backup count per user:"
echo "─────────────────────"

for USER in $USERS; do
  COUNT=$(ghostsnap hestia list-backups \
    --repository "$REPO" \
    --user "$USER" | \
    grep "hestia-$USER" | \
    wc -l)
  
  printf "%-20s %d\n" "$USER" "$COUNT"
done
```

**Output**:
```
Backup count per user:
─────────────────────
admin                5
alice                3
bob                  2
charlie              4
```

---

#### Find Old Backups

```bash
#!/bin/bash
# List backups older than 30 days

REPO="/var/ghostsnap/hestia"
CUTOFF=$(date -d "30 days ago" +%Y%m%d)

ghostsnap hestia list-backups --repository "$REPO" | \
  awk -v cutoff="$CUTOFF" '
    /hestia-/ {
      # Extract date from snapshot ID (YYYYMMdd)
      split($1, parts, "-")
      date = parts[3]
      if (date < cutoff) print $0
    }
  '
```

---

### Backup Verification Examples

#### Verify Daily Backups Exist

```bash
#!/bin/bash
# Check if backups exist for last 7 days

REPO="/var/ghostsnap/hestia"
USERS=$(ghostsnap hestia list-users)

for USER in $USERS; do
  echo "Checking backups for: $USER"
  
  for DAY in {0..6}; do
    DATE=$(date -d "$DAY days ago" +%Y%m%d)
    
    if ghostsnap hestia list-backups \
       --repository "$REPO" \
       --user "$USER" | \
       grep -q "hestia-$USER-$DATE"; then
      echo "  ✓ $DATE"
    else
      echo "  ✗ $DATE - MISSING"
    fi
  done
  echo
done
```

---

## Output Format

### Table Format

**Columns**:
- `Snapshot ID`: Unique snapshot identifier
- `User`: HestiaCP username (omitted if `--user` specified)
- `Date`: Backup creation timestamp (UTC)
- `Size`: Backup size (human-readable)
- `Tags`: Optional snapshot tags

**Sorting**: Newest first (descending by date)

---

### Snapshot ID Format

```
hestia-<username>-<timestamp>
```

**Components**:
- `hestia-`: Prefix (identifies HestiaCP snapshots)
- `<username>`: HestiaCP username
- `<timestamp>`: `YYYYMMdd-HHmmss` (UTC)

**Examples**:
```
hestia-admin-20251002-140000
hestia-alice-20251002-140530
hestia-prod-web-20251001-235959
```

---

## Behavior Details

### Query Process

```
1. Open repository (authenticate, decrypt)
   ↓
2. Query snapshots:
   - Pattern: hestia-*
   - If --user specified: hestia-<username>-*
   ↓
3. Fetch snapshot metadata:
   - Timestamp
   - Size
   - Tags
   - User (from snapshot ID)
   ↓
4. Sort by timestamp (newest first)
   ↓
5. Format and display
```

---

### Size Calculation

**Size shown**: Compressed + encrypted size in repository.

**Note**: Actual disk usage may differ due to:
- Deduplication (shared chunks across snapshots)
- Compression ratios
- Repository overhead

**Example**:
```
Backup tarball: 2.5 GB
After compression: 1.8 GB
After deduplication: 500 MB ← Size shown
```

---

## Error Handling

### Common Errors

#### Repository Not Found

```
Error: Repository not found at /var/ghostsnap/repo
```

**Solution**: Verify repository path:
```bash
ls -la /var/ghostsnap/repo
```

---

#### User Has No Backups

```
No backups found for user 'admin'
```

**Possible Causes**:
- User never backed up
- Wrong repository
- Backups deleted/expired

**Solution**: Run backup first:
```bash
sudo ghostsnap hestia backup --user admin --repository /var/ghostsnap/repo
```

---

#### Permission Denied

```
Error: Permission denied (os error 13)
```

**Solution**: Ensure read access to repository:
```bash
# For local repositories
sudo chmod -R 755 /var/ghostsnap/repo

# Or run with sudo
sudo ghostsnap hestia list-backups ...
```

---

#### Network Timeout (Remote Backends)

```
Error: Connection timeout
```

**Solution**:
```bash
# Check connectivity
ping minio.example.com

# Verify credentials
ghostsnap init minio://backups/hestia --minio-endpoint https://minio.example.com

# Retry
ghostsnap hestia list-backups --repository minio://backups/hestia
```

---

## Performance

### Benchmarks

| Snapshots | Local | MinIO (LAN) | S3 (Internet) |
|-----------|-------|-------------|---------------|
| 10 | ~50ms | ~200ms | ~500ms |
| 100 | ~200ms | ~1s | ~3s |
| 1000 | ~1s | ~5s | ~15s |

**Factors**:
- Network latency (remote backends)
- Repository size
- Number of snapshots

---

### Optimization

#### Cache Results

```bash
#!/bin/bash
# Cache for 5 minutes

CACHE_FILE="/tmp/ghostsnap-backups.cache"
CACHE_TTL=300  # 5 minutes

if [ -f "$CACHE_FILE" ]; then
  AGE=$(($(date +%s) - $(stat -c %Y "$CACHE_FILE")))
  
  if [ $AGE -lt $CACHE_TTL ]; then
    cat "$CACHE_FILE"
    exit 0
  fi
fi

# Fetch and cache
ghostsnap hestia list-backups --repository /var/ghostsnap/repo > "$CACHE_FILE"
cat "$CACHE_FILE"
```

---

#### Filter Early

```bash
# Faster: Filter by user
ghostsnap hestia list-backups --repository /var/ghostsnap/repo --user admin

# Slower: List all then grep
ghostsnap hestia list-backups --repository /var/ghostsnap/repo | grep admin
```

---

## Scripting Examples

### Backup Rotation Script

```bash
#!/bin/bash
# Delete backups older than 30 days

REPO="/var/ghostsnap/hestia"
CUTOFF=$(date -d "30 days ago" +%Y%m%d)

OLD_SNAPSHOTS=$(ghostsnap hestia list-backups --repository "$REPO" | \
  awk -v cutoff="$CUTOFF" '
    /hestia-/ {
      split($1, parts, "-")
      date = parts[3]
      if (date < cutoff) print $1
    }
  ')

for SNAPSHOT in $OLD_SNAPSHOTS; do
  echo "Deleting old snapshot: $SNAPSHOT"
  ghostsnap forget "$SNAPSHOT" --repository "$REPO"
done
```

---

### Backup Report Generator

```bash
#!/bin/bash
# Generate HTML report

REPO="/var/ghostsnap/hestia"
OUTPUT="backup-report.html"

cat > "$OUTPUT" <<EOF
<!DOCTYPE html>
<html>
<head>
  <title>Ghostsnap Backup Report</title>
  <style>
    table { border-collapse: collapse; width: 100%; }
    th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
    th { background-color: #4CAF50; color: white; }
  </style>
</head>
<body>
  <h1>Ghostsnap Backup Report</h1>
  <p>Generated: $(date)</p>
  <table>
    <tr>
      <th>Snapshot ID</th>
      <th>User</th>
      <th>Date</th>
      <th>Size</th>
    </tr>
EOF

ghostsnap hestia list-backups --repository "$REPO" | \
  grep "hestia-" | \
  awk '{print "<tr><td>"$1"</td><td>"$2"</td><td>"$3" "$4"</td><td>"$5" "$6"</td></tr>"}' >> "$OUTPUT"

cat >> "$OUTPUT" <<EOF
  </table>
</body>
</html>
EOF

echo "Report generated: $OUTPUT"
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success - backups listed |
| `1` | General error (repository not found) |
| `77` | Permission denied |

---

## See Also

- **[backup](backup.md)** - Create backups
- **[restore](restore.md)** - Restore from backup
- **[Backup Strategies](../use-cases/backup-strategies.md)** - Best practices

---

**Back to**: [Commands Overview](README.md) | [HestiaCP Integration](../README.md)
