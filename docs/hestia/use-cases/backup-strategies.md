# Backup Strategies for HestiaCP with Ghostsnap

Best practices and strategies for backing up HestiaCP servers with Ghostsnap.

---

## Table of Contents

- [The 3-2-1 Backup Rule](#the-3-2-1-backup-rule)
- [Backup Frequency](#backup-frequency)
- [Retention Policies](#retention-policies)
- [Storage Backends](#storage-backends)
- [Performance Optimization](#performance-optimization)
- [Disaster Recovery Planning](#disaster-recovery-planning)
- [Cost Optimization](#cost-optimization)

---

## The 3-2-1 Backup Rule

The **3-2-1 rule** is the gold standard for backup strategies:

### 3 Copies of Your Data
1. **Production data** (live HestiaCP server)
2. **Local backup** (fast recovery)
3. **Remote backup** (disaster recovery)

### 2 Different Media Types
- **Local**: Fast NAS/SAN storage
- **Cloud**: MinIO, S3, Azure

### 1 Off-Site Copy
- **Geographic separation** protects against:
  - Natural disasters
  - Physical theft
  - Data center failures

---

### Implementation Example

```bash
#!/bin/bash
# Daily backup to multiple destinations

# 1. Local backup (fast recovery)
ghostsnap hestia backup \
  --repository /var/ghostsnap/local \
  --cleanup --keep-tarballs 7

# 2. NAS backup (onsite)
ghostsnap hestia backup \
  --repository /mnt/nas/ghostsnap \
  --cleanup --keep-tarballs 14

# 3. Cloud backup (offsite)
ghostsnap hestia backup \
  --repository minio://backups/hestia \
  --cleanup --keep-tarballs 30
```

---

## Backup Frequency

Choose backup frequency based on **data change rate** and **Recovery Point Objective (RPO)**.

### Recommended Schedules

| Use Case | Frequency | Retention | Schedule |
|----------|-----------|-----------|----------|
| Production sites | Daily | 30 days | 2:00 AM |
| Development sites | Weekly | 8 weeks | Sunday 3:00 AM |
| High-traffic sites | Hourly | 7 days | Every hour |
| Static sites | Weekly | 12 weeks | Sunday 4:00 AM |
| Critical apps | Every 6 hours | 14 days | 00:00, 06:00, 12:00, 18:00 |

---

### Daily Backups

**Best for**: Most production environments.

```bash
# /etc/cron.d/ghostsnap-daily
0 2 * * * root /usr/local/bin/ghostsnap hestia backup \
  --repository /var/ghostsnap/daily \
  --cleanup --keep-tarballs 30 \
  2>&1 | logger -t ghostsnap
```

**Advantages**:
- ✅ Maximum 24-hour data loss
- ✅ Predictable schedule
- ✅ Reasonable storage requirements

**Disadvantages**:
- ❌ Up to 24 hours of data loss possible
- ❌ May not be suitable for high-change environments

---

### Hourly Backups

**Best for**: High-traffic e-commerce, databases with frequent updates.

```bash
# /etc/cron.d/ghostsnap-hourly
0 * * * * root /usr/local/bin/ghostsnap hestia backup \
  --repository /var/ghostsnap/hourly \
  --cleanup --keep-tarballs 168 \
  2>&1 | logger -t ghostsnap
```

**Advantages**:
- ✅ Maximum 1-hour data loss
- ✅ More granular recovery points

**Disadvantages**:
- ❌ Higher storage requirements
- ❌ More frequent backups (CPU/IO impact)

---

### Weekly Backups

**Best for**: Static sites, development environments, staging servers.

```bash
# /etc/cron.weekly/ghostsnap-backup
#!/bin/bash
/usr/local/bin/ghostsnap hestia backup \
  --repository /var/ghostsnap/weekly \
  --exclude "test-*" \
  --exclude "dev-*" \
  --cleanup --keep-tarballs 12
```

**Advantages**:
- ✅ Lower storage requirements
- ✅ Minimal performance impact

**Disadvantages**:
- ❌ Up to 7 days of data loss
- ❌ Not suitable for production

---

## Retention Policies

Balance **recovery needs** with **storage costs**.

### Grandfather-Father-Son (GFS)

**Strategy**: Keep backups at different intervals.

```
Daily:   7 days   (Son)
Weekly:  4 weeks  (Father)
Monthly: 12 months (Grandfather)
```

**Implementation**:

```bash
#!/bin/bash
# /usr/local/bin/gfs-backup.sh

DAY=$(date +%u)  # 1-7 (Monday-Sunday)
DATE=$(date +%d) # 01-31

# Daily backup (keep 7 days)
ghostsnap hestia backup \
  --repository /var/ghostsnap/daily \
  --cleanup --keep-tarballs 7

# Weekly backup (Sunday, keep 4 weeks)
if [ "$DAY" -eq 7 ]; then
  ghostsnap hestia backup \
    --repository /var/ghostsnap/weekly \
    --cleanup --keep-tarballs 4
fi

# Monthly backup (1st of month, keep 12 months)
if [ "$DATE" -eq 01 ]; then
  ghostsnap hestia backup \
    --repository /var/ghostsnap/monthly \
    --cleanup --keep-tarballs 12
fi
```

**Cron**:
```
0 2 * * * /usr/local/bin/gfs-backup.sh
```

---

### Simple Retention

**Strategy**: Keep N most recent backups.

```bash
# Keep last 30 daily backups
ghostsnap hestia backup \
  --repository /var/ghostsnap/repo \
  --cleanup --keep-tarballs 30
```

**Advantages**:
- ✅ Simple to understand
- ✅ Predictable storage usage

**Disadvantages**:
- ❌ All backups have same age
- ❌ No long-term history

---

### Time-Based Retention

**Strategy**: Delete backups older than X days.

```bash
#!/bin/bash
# Delete backups older than 90 days

REPO="/var/ghostsnap/repo"
CUTOFF=$(date -d "90 days ago" +%Y%m%d)

OLD_SNAPSHOTS=$(ghostsnap hestia list-backups --repository "$REPO" | \
  awk -v cutoff="$CUTOFF" '
    /hestia-/ {
      split($1, parts, "-")
      date = parts[3]
      if (date < cutoff) print $1
    }
  ')

for SNAPSHOT in $OLD_SNAPSHOTS; do
  echo "Deleting: $SNAPSHOT"
  ghostsnap forget "$SNAPSHOT" --repository "$REPO"
done
```

---

## Storage Backends

Choose backends based on **recovery speed**, **cost**, and **durability** requirements.

### Comparison Table

| Backend | Speed | Cost | Durability | Use Case |
|---------|-------|------|------------|----------|
| **Local Disk** | ⚡ Fastest | 💰 Low | ⚠️ Single point of failure | Fast recovery |
| **NAS/NFS** | ⚡ Fast | 💰💰 Medium | ✅ Good | Primary backup |
| **MinIO** | 🔥 Fast | 💰💰 Medium | ✅ Excellent | S3-compatible local |
| **S3** | 🐌 Slow | 💰💰💰 High | ✅ Excellent | Long-term offsite |
| **Azure Blob** | 🐌 Slow | 💰💰💰 High | ✅ Excellent | Enterprise offsite |

---

### Local Disk Strategy

**Best for**: Fast recovery, temporary staging.

```bash
# Initialize
ghostsnap init /var/ghostsnap/local

# Backup
ghostsnap hestia backup \
  --repository /var/ghostsnap/local \
  --cleanup --keep-tarballs 7
```

**⚠️ WARNING**: Not suitable as sole backup (single point of failure).

**Recommendation**: Use as first tier in multi-tier strategy.

---

### NAS/NFS Strategy

**Best for**: Primary backup with good performance and durability.

```bash
# Mount NFS share
sudo mount -t nfs nas.local:/volume1/ghostsnap /mnt/nas

# Make permanent (add to /etc/fstab)
echo "nas.local:/volume1/ghostsnap /mnt/nas nfs defaults 0 0" | \
  sudo tee -a /etc/fstab

# Initialize
ghostsnap init /mnt/nas/ghostsnap

# Backup
ghostsnap hestia backup \
  --repository /mnt/nas/ghostsnap \
  --cleanup --keep-tarballs 14
```

**Advantages**:
- ✅ Fast local network speeds
- ✅ Centralized storage
- ✅ Good durability (RAID)

---

### MinIO Strategy

**Best for**: S3-compatible local object storage.

```bash
# Initialize
ghostsnap init minio://backups/hestia \
  --minio-endpoint https://minio.example.com \
  --minio-access-key AKIAIOSFODNN7EXAMPLE \
  --minio-secret-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

# Backup
ghostsnap hestia backup \
  --repository minio://backups/hestia \
  --cleanup --keep-tarballs 30
```

**Advantages**:
- ✅ S3-compatible API
- ✅ Versioning support
- ✅ Cost-effective
- ✅ Self-hosted control

---

### S3 Strategy

**Best for**: Long-term offsite storage, compliance requirements.

```bash
# Initialize
ghostsnap init s3://my-backup-bucket/hestia

# Backup
ghostsnap hestia backup \
  --repository s3://my-backup-bucket/hestia \
  --cleanup --keep-tarballs 90
```

**Cost Optimization**:
```bash
# Use S3 Intelligent-Tiering
aws s3api put-bucket-intelligent-tiering-configuration \
  --bucket my-backup-bucket \
  --id auto-archive \
  --intelligent-tiering-configuration file://tiering.json
```

**tiering.json**:
```json
{
  "Id": "auto-archive",
  "Status": "Enabled",
  "Tierings": [
    {
      "Days": 90,
      "AccessTier": "ARCHIVE_ACCESS"
    },
    {
      "Days": 180,
      "AccessTier": "DEEP_ARCHIVE_ACCESS"
    }
  ]
}
```

---

## Performance Optimization

### Parallel Backups

Backup multiple users in parallel for faster completion:

```bash
#!/bin/bash
# Parallel backup with GNU parallel

USERS=$(ghostsnap hestia list-users)

echo "$USERS" | parallel -j 4 \
  sudo ghostsnap hestia backup \
    --user {} \
    --repository /var/ghostsnap/repo \
    --cleanup
```

**⚠️ NOTE**: Monitor system resources (CPU, I/O, network).

---

### Exclude Patterns

Skip unnecessary files/users:

```bash
# Skip test users
ghostsnap hestia backup \
  --repository /var/ghostsnap/repo \
  --exclude "test-*" \
  --exclude "*-dev" \
  --cleanup
```

---

### Schedule During Off-Peak

```bash
# 2 AM when traffic is lowest
0 2 * * * /usr/local/bin/ghostsnap hestia backup ...
```

---

### Incremental Backups

Ghostsnap automatically uses content-addressed storage for deduplication:

```
First backup:  10 GB (full)
Second backup: 2 GB  (only changes)
Third backup:  1.5 GB (only changes)
```

**Savings**: ~80-90% for subsequent backups.

---

## Disaster Recovery Planning

### Define Objectives

#### Recovery Time Objective (RTO)
**How long can you be down?**

| RTO | Strategy |
|-----|----------|
| < 1 hour | Local backups + hot standby |
| 1-4 hours | Local backups + documented recovery |
| 4-24 hours | Cloud backups + recovery runbook |
| > 24 hours | Weekly backups acceptable |

#### Recovery Point Objective (RPO)
**How much data can you lose?**

| RPO | Backup Frequency |
|-----|------------------|
| 0 (zero data loss) | Real-time replication |
| < 1 hour | Hourly backups |
| < 24 hours | Daily backups |
| < 7 days | Weekly backups |

---

### Recovery Runbook

Document step-by-step recovery procedures:

```markdown
# Disaster Recovery Procedure

## Scenario: Complete Server Failure

### 1. Provision New Server
- Deploy HestiaCP on fresh Ubuntu 22.04
- Same hostname and IP (if possible)
- Install Ghostsnap

### 2. Restore System Backups
```bash
# List available backups
ghostsnap hestia list-backups --repository /mnt/nas/ghostsnap

# Restore all users
for USER in $(ghostsnap hestia list-users); do
  sudo ghostsnap hestia restore $USER \
    --repository /mnt/nas/ghostsnap \
    --force
done
```

### 3. Rebuild HestiaCP
```bash
# Rebuild all services
for USER in $(ghostsnap hestia list-users); do
  sudo v-rebuild-user $USER
  sudo v-rebuild-web-domains $USER
  sudo v-rebuild-dns-domains $USER
  sudo v-rebuild-mail-domains $USER
done
```

### 4. Verify Services
```bash
# Test websites
curl -I https://example.com

# Test mail
telnet localhost 25

# Test databases
mysql -u root -p -e "SHOW DATABASES;"
```

### 5. Update DNS (if IP changed)
- Update A records
- Wait for TTL propagation
- Verify with `dig`

### 6. Monitor
- Check error logs
- Monitor resource usage
- Verify backup integrity
```

---

## Cost Optimization

### Storage Tier Selection

| Data Age | Tier | Cost | Retrieval Time |
|----------|------|------|----------------|
| 0-7 days | Hot | $$$ | Instant |
| 7-30 days | Warm | $$ | Minutes |
| 30-90 days | Cool | $ | Hours |
| 90+ days | Archive | ¢ | 1-5 hours |

---

### Deduplication Benefits

Ghostsnap's content-addressed storage saves significant space:

**Example**: 30 daily backups of 10 GB each

```
Without deduplication: 300 GB
With deduplication:    ~50 GB (83% savings)
```

**Factors**:
- Static content (images, libraries) deduplicated 100%
- Database dumps deduplicated ~70%
- Log files deduplicated ~50%

---

### Compression Ratios

Typical compression with ZSTD:

| Content Type | Compression Ratio |
|--------------|------------------|
| Text files | 70-80% |
| HTML/CSS/JS | 60-70% |
| Images (JPEG/PNG) | 5-10% (already compressed) |
| Databases | 60-70% |
| Log files | 80-90% |

**Average**: ~60% compression across mixed content.

---

## Example Strategies

### Small Business (1-10 Users)

```bash
# Daily backups to NAS
0 2 * * * ghostsnap hestia backup \
  --repository /mnt/nas/ghostsnap \
  --cleanup --keep-tarballs 30

# Weekly backups to cloud
0 3 * * 0 ghostsnap hestia backup \
  --repository s3://backups/hestia \
  --cleanup --keep-tarballs 12
```

**Cost**: ~$20/month (S3)
**RTO**: 4 hours
**RPO**: 24 hours

---

### Enterprise (100+ Users)

```bash
# Hourly backups to local
0 * * * * ghostsnap hestia backup \
  --repository /var/ghostsnap/hourly \
  --cleanup --keep-tarballs 168

# Daily backups to MinIO
0 2 * * * ghostsnap hestia backup \
  --repository minio://backups/daily \
  --cleanup --keep-tarballs 30

# Monthly backups to S3 Glacier
0 4 1 * * ghostsnap hestia backup \
  --repository s3://backups/monthly \
  --cleanup --keep-tarballs 24
```

**Cost**: ~$500/month (MinIO + S3)
**RTO**: 1 hour
**RPO**: 1 hour

---

## See Also

- **[Automation Guide](automation.md)** - Systemd and cron setup
- **[Disaster Recovery](disaster-recovery.md)** - Recovery procedures
- **[Troubleshooting](../advanced/troubleshooting.md)** - Common issues

---

**Back to**: [HestiaCP Integration](../README.md)
