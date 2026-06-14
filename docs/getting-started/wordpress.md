# Quick Start: WordPress Backup with Ghostsnap

Back up WordPress sites using standard Ghostsnap commands.

---

## Step 1: Build Ghostsnap

```bash
cd /data/projects/ghostsnap
cargo build --release

# Copy binary to system path
sudo cp target/release/ghostsnap /usr/local/bin/
```

## Step 2: Initialize Repository

```bash
# Set your repository password (save this somewhere safe!)
export GHOSTSNAP_PASSWORD="your-super-secure-password"

# Initialize local repository
ghostsnap init /backup/wordpress

# Or initialize with S3 storage
ghostsnap init --backend s3 --bucket my-backups --prefix wordpress s3:my-backups/wordpress
```

## Step 3: Back Up WordPress

```bash
# Dump the database first
mysqldump -u wpuser -p wordpress_db > /home/user/web/example.com/db_backup.sql

# Back up WordPress files including database dump
ghostsnap --repo /backup/wordpress backup /home/user/web/example.com/public_html \
    --tag wordpress \
    --exclude "wp-content/cache/*" \
    --exclude "*.log"

# Clean up database dump
rm /home/user/web/example.com/db_backup.sql
```

## Step 4: Schedule Automated Backups

Create `/root/wordpress-backup.sh`:

```bash
#!/bin/bash
set -e

export GHOSTSNAP_PASSWORD=$(cat /root/.ghostsnap-password)
REPO="/backup/wordpress"
WP_PATH="/home/user/web/example.com/public_html"
DATE=$(date +%Y-%m-%d)

# Dump database
mysqldump -u wpuser -p wordpress_db > "${WP_PATH}/db_backup.sql"

# Run backup
ghostsnap --repo "$REPO" backup "$WP_PATH" --tag wordpress --tag "$DATE"

# Clean up
rm "${WP_PATH}/db_backup.sql"

# Apply retention (keep 7 daily, 4 weekly)
ghostsnap --repo "$REPO" forget --keep-daily 7 --keep-weekly 4 --tag wordpress
ghostsnap --repo "$REPO" prune
```

Set up cron:

```bash
# Create password file
sudo bash -c 'echo "your-password" > /root/.ghostsnap-password'
sudo chmod 600 /root/.ghostsnap-password

# Add to crontab (2 AM daily)
0 2 * * * /root/wordpress-backup.sh >> /var/log/ghostsnap.log 2>&1
```

## Step 5: Restore

```bash
# List snapshots
ghostsnap --repo /backup/wordpress snapshots --tag wordpress

# Restore to temporary location
ghostsnap --repo /backup/wordpress restore abc123 --target /tmp/wordpress-restore

# Restore database
mysql -u wpuser -p wordpress_db < /tmp/wordpress-restore/home/user/web/example.com/public_html/db_backup.sql
```

---

## What You Get

- **Encrypted backups** - ChaCha20-Poly1305 encryption
- **Deduplication** - Only changed data is stored
- **Compression** - zlib compression on all chunks
- **Fast restore** - Chunked storage for partial restores
- **Storage backends** - Local filesystem or S3 (including S3-compatible providers)

---

## Troubleshooting

**"Repository not found"**
```bash
# Check GHOSTSNAP_REPO or use --repo flag
ghostsnap --repo /backup/wordpress snapshots
```

**"Invalid password"**
```bash
# Password must match what was used during init
export GHOSTSNAP_PASSWORD="correct-password"
```

**"Database dump failed"**
```bash
# Test mysql connection first
mysqldump -u wpuser -p wordpress_db > /dev/null
```

---

See [docs/hestia/](hestia/) for backing up HestiaCP-hosted WordPress sites.
