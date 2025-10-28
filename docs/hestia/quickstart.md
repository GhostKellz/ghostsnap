# HestiaCP Integration - Quick Start

Get up and running with HestiaCP backups in 5 minutes!

---

## Step 1: Verify HestiaCP Installation

```bash
# Check HestiaCP is installed
ls -la /usr/local/hestia

# List HestiaCP users
v-list-users
```

If HestiaCP is not installed, visit [hestiacp.com](https://hestiacp.com/) for installation instructions.

---

## Step 2: Install Ghostsnap

```bash
# From source (recommended)
git clone https://github.com/ghostkellz/ghostsnap
cd ghostsnap
cargo build --release
sudo cp target/release/ghostsnap /usr/local/bin/

# Verify installation
ghostsnap --version
```

---

## Step 3: Initialize Repository

Choose your backend and initialize a repository:

### Option A: MinIO (Recommended for self-hosted)

```bash
# Initialize repository with MinIO backend
ghostsnap init /var/ghostsnap/hestia \
  --backend minio \
  --endpoint http://localhost:9000 \
  --bucket hestia-backups \
  --access-key YOUR_ACCESS_KEY \
  --secret-key YOUR_SECRET_KEY
```

### Option B: Local Storage (Synology NFS)

```bash
# Mount Synology NFS share
sudo mount -t nfs 192.168.1.100:/volume1/backups /mnt/synology

# Initialize repository on NFS
ghostsnap init /mnt/synology/hestia-backups \
  --backend local
```

### Option C: AWS S3

```bash
# Initialize repository with S3
ghostsnap init /var/ghostsnap/hestia \
  --backend s3 \
  --bucket my-hestia-backups \
  --region us-east-1
```

---

## Step 4: List HestiaCP Users

```bash
# List all users with details
ghostsnap hestia list-users --detailed

# Example output:
# 📋 HestiaCP Users (5):
# ============================================================
#
# 👤 admin 🟢 ACTIVE
#    📁 Home: /home/admin
#    🌐 Domains: 3
#    🗄️  Databases: 2
#    💾 Disk: 1024.50 MB
#    📄 Domain list:
#      🔒 example.com
#      🔒 shop.example.com
#      🔓 test.example.com
```

---

## Step 5: Backup a Single User

```bash
# Backup single user with cleanup
sudo ghostsnap hestia backup \
  --user admin \
  --repository /var/ghostsnap/hestia \
  --cleanup

# Example output:
# 🚀 Starting backup for 1 user(s)
#
# [1/1] Backing up user: admin ...
#   📦 Creating HestiaCP backup...
#   📊 Tarball size: 128.45 MB
#   📁 Local tarball: /backup/admin.2025-10-02_14-00-00.tar
#   ⬆️  Uploading to Ghostsnap repository...
#   ✅ Backed up as snapshot: hestia-admin-20251002-140000
#   🧹 Cleaned up 2 old tarball(s)
# ✅ Successfully backed up user: admin
#
# 🎉 Backup Summary:
#    ✅ Successful: 1
#    ❌ Failed: 0
```

---

## Step 6: Backup All Users

```bash
# Backup all users
sudo ghostsnap hestia backup \
  --repository /var/ghostsnap/hestia \
  --cleanup \
  --keep-tarballs 3

# Exclude test users
sudo ghostsnap hestia backup \
  --repository /var/ghostsnap/hestia \
  --exclude "test-*" \
  --cleanup
```

---

## Step 7: List Backups

```bash
# List all HestiaCP backups
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/hestia

# Filter by user
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/hestia \
  --user admin
```

---

## Step 8: Restore a User (If Needed)

```bash
# Restore to temporary location first
ghostsnap hestia restore admin \
  --snapshot hestia-admin-20251002-140000 \
  --repository /var/ghostsnap/hestia \
  --temp

# Review the restored tarball
tar -tzf /tmp/ghostsnap-restore-admin.tar | head -20

# Apply to HestiaCP
v-restore-user admin /tmp/ghostsnap-restore-admin.tar
```

---

## Step 9: Automate with Cron

```bash
# Create backup script
sudo tee /usr/local/bin/backup-hestia.sh > /dev/null <<'EOF'
#!/bin/bash
ghostsnap hestia backup \
  --repository /var/ghostsnap/hestia \
  --cleanup \
  --keep-tarballs 3
EOF

# Make executable
sudo chmod +x /usr/local/bin/backup-hestia.sh

# Add to crontab (daily at 2 AM)
echo "0 2 * * * /usr/local/bin/backup-hestia.sh" | sudo crontab -
```

---

## Step 10: Verify Everything Works

```bash
# Check user info
ghostsnap hestia user-info admin

# List users
ghostsnap hestia list-users

# Check backups
ghostsnap hestia list-backups --repository /var/ghostsnap/hestia
```

---

## 🎉 You're Done!

Your HestiaCP server is now backed up to Ghostsnap!

### What Happens Now?

1. **Daily Backups** - Cron runs backup script every day at 2 AM
2. **Automatic Cleanup** - Old tarballs are removed (keeping last 3)
3. **Encrypted Storage** - All data encrypted with ChaCha20-Poly1305
4. **Deduplication** - Common files stored only once
5. **Multiple Backends** - Data stored in MinIO, S3, or local storage

### Next Steps

- **[Backup Strategies](use-cases/backup-strategies.md)** - Learn best practices
- **[Automation Guide](use-cases/automation.md)** - Advanced scheduling
- **[Disaster Recovery](use-cases/disaster-recovery.md)** - Recovery procedures
- **[CLI Reference](commands/README.md)** - All available commands

---

## Common Next Actions

### Monitor Backups

```bash
# Check cron logs
sudo tail -f /var/log/cron

# Check Ghostsnap logs
sudo journalctl -u ghostsnap -f
```

### Test Restore

```bash
# Create test user
v-add-user testuser testpassword test@example.com

# Backup test user
sudo ghostsnap hestia backup --user testuser --repository /var/ghostsnap/hestia --cleanup

# Delete test user
v-delete-user testuser

# Restore test user
ghostsnap hestia restore testuser \
  --snapshot hestia-testuser-YYYYMMDD-HHMMSS \
  --repository /var/ghostsnap/hestia

# Verify restoration
v-list-users | grep testuser
```

### Multi-Destination Backups (3-2-1 Strategy)

```bash
# Primary: MinIO
sudo ghostsnap hestia backup --repository /var/ghostsnap/minio --cleanup

# Secondary: Synology NFS
sudo ghostsnap hestia backup --repository /mnt/synology/hestia

# Tertiary: AWS S3 (offsite)
sudo ghostsnap hestia backup --repository /var/ghostsnap/s3 --cleanup
```

---

## Troubleshooting

### "User not found" Error

```bash
# List available users
ghostsnap hestia list-users

# Check HestiaCP users
v-list-users
```

### "Permission denied" Error

```bash
# HestiaCP commands require root
sudo ghostsnap hestia backup ...
```

### "Repository not found" Error

```bash
# Initialize repository first
ghostsnap init /var/ghostsnap/hestia --backend local
```

### Disk Space Issues

```bash
# Check disk space
df -h /backup

# Clean up old tarballs manually
sudo rm /backup/*.tar

# Or use cleanup option
sudo ghostsnap hestia backup ... --cleanup --keep-tarballs 1
```

---

## Need Help?

- **Full Documentation**: [README.md](README.md)
- **CLI Reference**: [commands/README.md](commands/README.md)
- **Troubleshooting**: [advanced/troubleshooting.md](advanced/troubleshooting.md)
- **GitHub Issues**: https://github.com/ghostkellz/ghostsnap/issues

---

**You're all set! Happy backing up! 🚀**
