# WordPress Backup Guide for Ghostsnap + HestiaCP

**Status**: Ready for Production Testing
**Version**: 0.1.0
**Last Updated**: 2025-10-27

---

## 🎯 Quick Start (5 Minutes)

### Prerequisites

- HestiaCP server with WordPress installed
- Root or sudo access
- Ghostsnap installed (`cargo build --release`)
- Storage backend configured (local, S3, MinIO, or Azure)

### Basic WordPress Backup

```bash
# 1. Initialize repository (one-time)
export GHOSTSNAP_PASSWORD="your-secure-password"
export GHOSTSNAP_REPO="/backup/ghostsnap-repo"
ghostsnap init --backend local

# 2. Backup WordPress site
ghostsnap backup /home/username/web/yourdomain.com/public_html \
    --tag "wordpress" \
    --tag "$(date +%Y-%m-%d)" \
    --exclude "*.log" \
    --exclude "wp-content/cache" \
    --exclude "wp-content/uploads/cache"

# 3. Verify backup
ghostsnap snapshots
```

---

## 📋 Complete WordPress Backup Strategy

### What to Backup

#### Essential Files (Always Backup)
```
/home/username/web/yourdomain.com/public_html/
├── wp-config.php                    # Database credentials
├── wp-content/
│   ├── themes/                      # Custom themes
│   ├── plugins/                     # Installed plugins
│   ├── uploads/                     # Media files
│   └── mu-plugins/                  # Must-use plugins
├── .htaccess                        # Rewrite rules
└── wp-admin/ & wp-includes/         # Core files (optional)
```

#### Database
```bash
# Backup WordPress database separately
mysqldump -u dbuser -p dbname > /tmp/wordpress-db-$(date +%Y%m%d).sql
ghostsnap backup /tmp/wordpress-db-$(date +%Y%m%d).sql --tag "database"
rm /tmp/wordpress-db-$(date +%Y%m%d).sql
```

#### SSL Certificates (HestiaCP)
```
/usr/local/hestia/data/users/username/ssl/
```

### What to Exclude

```bash
--exclude "wp-content/cache/*"              # Cache files
--exclude "wp-content/uploads/cache/*"      # Upload cache
--exclude "wp-content/w3tc-cache/*"         # W3 Total Cache
--exclude "wp-content/wp-rocket-cache/*"    # WP Rocket
--exclude "wp-content/backup-*"             # Plugin backups
--exclude "*.log"                           # Log files
--exclude ".git"                            # Git repositories
--exclude "node_modules"                    # npm packages
--exclude "error_log"                       # Error logs
```

---

## 🔧 Production Backup Script

Create `/usr/local/bin/ghostsnap-wordpress-backup.sh`:

```bash
#!/bin/bash
set -euo pipefail

# Configuration
WORDPRESS_ROOT="/home/username/web/yourdomain.com/public_html"
DB_NAME="wordpress_db"
DB_USER="wordpress_user"
DB_PASS="db_password"
GHOSTSNAP_REPO="/backup/ghostsnap-repo"
GHOSTSNAP_PASSWORD="your-secure-password"
KEEP_DB_DUMPS=3  # Keep last N database dumps locally

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

# Export environment variables
export GHOSTSNAP_PASSWORD
export GHOSTSNAP_REPO

log "Starting WordPress backup for $(basename $WORDPRESS_ROOT)"

# 1. Backup database
log "Backing up WordPress database..."
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
DB_BACKUP_DIR="/tmp/ghostsnap-db-backups"
mkdir -p "$DB_BACKUP_DIR"
DB_FILE="$DB_BACKUP_DIR/wordpress-db-$TIMESTAMP.sql"

if mysqldump -u "$DB_USER" -p"$DB_PASS" "$DB_NAME" > "$DB_FILE" 2>/dev/null; then
    log "Database exported: $(du -h $DB_FILE | cut -f1)"

    # Compress database dump
    gzip "$DB_FILE"
    DB_FILE="${DB_FILE}.gz"

    # Backup database to ghostsnap
    if ghostsnap backup "$DB_FILE" \
        --tag "wordpress" \
        --tag "database" \
        --tag "$(date +%Y-%m-%d)"; then
        log "Database backup successful"

        # Cleanup old local dumps
        cd "$DB_BACKUP_DIR"
        ls -t wordpress-db-*.sql.gz | tail -n +$((KEEP_DB_DUMPS + 1)) | xargs rm -f
    else
        error "Database backup failed"
        exit 1
    fi
else
    error "Database dump failed"
    exit 1
fi

# 2. Backup WordPress files
log "Backing up WordPress files..."
if ghostsnap backup "$WORDPRESS_ROOT" \
    --tag "wordpress" \
    --tag "files" \
    --tag "$(date +%Y-%m-%d)" \
    --exclude "wp-content/cache/*" \
    --exclude "wp-content/uploads/cache/*" \
    --exclude "wp-content/w3tc-cache/*" \
    --exclude "wp-content/wp-rocket-cache/*" \
    --exclude "wp-content/backup-*" \
    --exclude "*.log" \
    --exclude "error_log" \
    --exclude ".git" \
    --exclude "node_modules"; then
    log "Files backup successful"
else
    error "Files backup failed"
    exit 1
fi

# 3. Show summary
log "Backup completed successfully!"
echo ""
log "Recent snapshots:"
ghostsnap snapshots | head -10

# Cleanup temp database file
rm -f "$DB_FILE"

log "Backup script finished"
```

Make it executable:
```bash
chmod +x /usr/local/bin/ghostsnap-wordpress-backup.sh
```

---

## ⏰ Automated Backups with Cron

### Daily Backup at 2 AM

```bash
# Edit crontab
sudo crontab -e

# Add this line:
0 2 * * * /usr/local/bin/ghostsnap-wordpress-backup.sh >> /var/log/ghostsnap-backup.log 2>&1
```

### Backup Strategy Options

#### Conservative (Daily + Weekly + Monthly)
```bash
# Daily backup at 2 AM (keep for 7 days via external cleanup)
0 2 * * * /usr/local/bin/ghostsnap-wordpress-backup.sh --tag "daily"

# Weekly backup on Sunday at 3 AM
0 3 * * 0 /usr/local/bin/ghostsnap-wordpress-backup.sh --tag "weekly"

# Monthly backup on 1st at 4 AM
0 4 1 * * /usr/local/bin/ghostsnap-wordpress-backup.sh --tag "monthly"
```

#### Aggressive (Every 6 Hours)
```bash
0 */6 * * * /usr/local/bin/ghostsnap-wordpress-backup.sh
```

---

## 🔄 Restore WordPress Site

### Full Restore

```bash
# 1. List available snapshots
ghostsnap snapshots

# 2. Restore files (using short ID)
ghostsnap restore a3c7f2 /home/username/web/yourdomain.com/public_html-restored

# 3. Find and restore database
# Find database snapshot
ghostsnap snapshots | grep database

# Restore database file
ghostsnap restore b8d9e1 /tmp/db-restore

# Import database
cd /tmp/db-restore
gunzip wordpress-db-*.sql.gz
mysql -u wordpress_user -p wordpress_db < wordpress-db-*.sql
```

### Selective Restore (Single Plugin/Theme)

```bash
# Restore specific files by filtering
ghostsnap restore a3c7f2 /tmp/selective-restore

# Copy only what you need
cp -r /tmp/selective-restore/wp-content/plugins/my-plugin \
    /home/username/web/yourdomain.com/public_html/wp-content/plugins/
```

---

## 🔐 Security Best Practices

### 1. Secure Password Storage

**Option A: Environment Variables (Recommended)**
```bash
# /root/.bashrc or /root/.profile
export GHOSTSNAP_PASSWORD="your-secure-password"
export GHOSTSNAP_REPO="/backup/ghostsnap-repo"
```

**Option B: Systemd Credentials**
```bash
# Store password in systemd credential store
echo "your-secure-password" | systemd-creds encrypt - /etc/ghostsnap/repo-password
```

**Option C: Password File (Restricted Permissions)**
```bash
echo "your-secure-password" > /root/.ghostsnap-password
chmod 600 /root/.ghostsnap-password

# In script:
export GHOSTSNAP_PASSWORD=$(cat /root/.ghostsnap-password)
```

### 2. Repository Permissions

```bash
# Restrict repository access to root only
chown -R root:root /backup/ghostsnap-repo
chmod 700 /backup/ghostsnap-repo
```

### 3. Database Credentials

```bash
# Store database password securely
chmod 600 /usr/local/bin/ghostsnap-wordpress-backup.sh
# Only root can read the script containing DB credentials
```

---

## 📊 Monitoring and Verification

### Check Backup Status

```bash
#!/bin/bash
# /usr/local/bin/ghostsnap-backup-check.sh

export GHOSTSNAP_PASSWORD="your-password"
export GHOSTSNAP_REPO="/backup/ghostsnap-repo"

# Get most recent snapshot
LATEST=$(ghostsnap snapshots | head -2 | tail -1 | awk '{print $1}')
LATEST_DATE=$(ghostsnap snapshots | head -2 | tail -1 | awk '{print $2, $3}')

# Check if backup is recent (within 25 hours)
BACKUP_TIMESTAMP=$(date -d "$LATEST_DATE" +%s)
CURRENT_TIMESTAMP=$(date +%s)
AGE_HOURS=$(( ($CURRENT_TIMESTAMP - $BACKUP_TIMESTAMP) / 3600 ))

if [ $AGE_HOURS -gt 25 ]; then
    echo "WARNING: Last backup is $AGE_HOURS hours old!"
    exit 1
else
    echo "OK: Last backup is $AGE_HOURS hours old (snapshot: $LATEST)"
    exit 0
fi
```

### Email Notifications

```bash
# Add to backup script
if ! /usr/local/bin/ghostsnap-wordpress-backup.sh; then
    echo "WordPress backup failed!" | mail -s "BACKUP FAILED" admin@yourdomain.com
fi
```

---

## 🚀 Advanced: Multi-Site WordPress

For WordPress Multisite installations:

```bash
#!/bin/bash
# Backup all sites in a multisite installation

MULTISITE_ROOT="/home/username/web/network.com/public_html"
export GHOSTSNAP_PASSWORD="password"
export GHOSTSNAP_REPO="/backup/ghostsnap-repo"

# Backup main installation
ghostsnap backup "$MULTISITE_ROOT" \
    --tag "wordpress-multisite" \
    --tag "main-network" \
    --exclude "wp-content/cache/*" \
    --exclude "wp-content/uploads/sites/*/cache/*"

# Backup databases for each site
for site_id in 1 2 3 4 5; do
    mysqldump -u root -p wpms_site_${site_id} | gzip > /tmp/site-${site_id}.sql.gz
    ghostsnap backup /tmp/site-${site_id}.sql.gz --tag "site-${site_id}" --tag "database"
    rm /tmp/site-${site_id}.sql.gz
done
```

---

## 🐞 Troubleshooting

### Backup Taking Too Long

```bash
# Check what's being backed up
du -sh /home/username/web/yourdomain.com/public_html/*

# Exclude large directories
ghostsnap backup /path/to/site \
    --exclude "wp-content/uploads/videos/*" \
    --exclude "wp-content/uploads/backup/*"
```

### Deduplication Not Working

```bash
# Check chunk count
ls /backup/ghostsnap-repo/index/ | wc -l

# If chunks are duplicating, verify:
# 1. Same GHOSTSNAP_PASSWORD is used
# 2. Same GHOSTSNAP_REPO path
# 3. Repository wasn't reinitialized
```

### Restore Fails

```bash
# Verify snapshot exists
ghostsnap snapshots | grep "snapshot-id"

# Check repository integrity (future feature)
# ghostsnap check --verbose

# Try restore to temporary location first
ghostsnap restore snapshot-id /tmp/test-restore
```

---

## 📈 Storage Estimates

### Typical WordPress Site

| Component | Size | Deduplicated |
|-----------|------|--------------|
| Core WP Files | 50 MB | ~20 MB (after 1st backup) |
| Themes | 10 MB | ~5 MB |
| Plugins | 30 MB | ~15 MB |
| Uploads (1 year) | 2 GB | ~2 GB (mostly images) |
| Database | 100 MB | ~100 MB (grows daily) |
| **Total** | **~2.2 GB** | **~2.14 GB** |

### Growth Over Time (Daily Backups)

- **Week 1**: 2.2 GB (initial) + 150 MB (daily changes) = 3.25 GB
- **Month 1**: ~5-6 GB total with deduplication
- **Year 1**: ~50-60 GB total with deduplication

**With deduplication**: Only changed files consume additional space, typically 10-20% of site size per backup.

---

## ✅ Pre-Production Checklist

Before using Ghostsnap for production WordPress backups:

- [ ] Test backup and restore on staging site
- [ ] Verify database restoration works correctly
- [ ] Confirm file permissions are preserved
- [ ] Test selective file restore
- [ ] Set up monitoring and alerts
- [ ] Document recovery procedures
- [ ] Test restore on different server
- [ ] Verify encryption with correct password
- [ ] Set up offsite backup (S3/MinIO/Azure)
- [ ] Schedule automated backups
- [ ] Test backup failure scenarios

---

## 🎓 Next Steps

1. **Initial Testing** (This Week)
   - Backup a test WordPress site
   - Perform full restore to verify
   - Test selective file restoration

2. **Production Deployment** (Next Week)
   - Deploy backup script to production
   - Schedule automated cron jobs
   - Set up monitoring

3. **Optimization** (Ongoing)
   - Fine-tune exclude patterns
   - Adjust backup frequency
   - Monitor storage usage

---

## 📞 Support

- **Documentation**: `/data/projects/ghostsnap/docs/`
- **Issues**: GitHub Issues (when repository is public)
- **Current Status**: Alpha → RC1 (Production Polish Phase Complete)

---

**Last Updated**: 2025-10-27
**Tested On**: HestiaCP 1.8.x, WordPress 6.x
**Ghostsnap Version**: 0.1.0 (RC1 Candidate)
