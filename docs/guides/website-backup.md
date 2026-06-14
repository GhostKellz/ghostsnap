# Website Backup Guide

This guide covers backing up and restoring a typical self-hosted website stack with Ghostsnap:

- Ubuntu/Debian host with Nginx
- Site content under `/var/www`
- TLS certificates (Let's Encrypt)
- Database dumps (MySQL/PostgreSQL)
- Offsite storage to B2 or Azure

## What to Back Up

### Essential Paths

| Path | Description |
|------|-------------|
| `/etc/nginx/` | Nginx configuration |
| `/var/www/` | Site content and applications |
| `/etc/letsencrypt/` | TLS certificates and keys |
| `~/.env` files | Application secrets |

### Database Dumps

Do not back up live database directories. Instead, dump databases to a staging location before backup.

**MySQL/MariaDB:**
```bash
mysqldump --defaults-extra-file=/root/.my.cnf --single-transaction dbname > /var/backups/ghostsnap/dbname.sql
```

**PostgreSQL:**
```bash
pg_dump -Fc myapp > /var/backups/ghostsnap/myapp.dump
```

### Staging Directory

Create a dedicated staging directory for database dumps and other generated files:

```bash
mkdir -p /var/backups/ghostsnap/web-01
```

## Job Configuration

### Example: Website to Backblaze B2

Create `/etc/ghostsnap/jobs.toml`:

```toml
version = 1

[defaults]
password_file = "/etc/ghostsnap/password"

[jobs.website-b2]
repository = "s3:my-b2-bucket/web-01"
paths = ["/etc/nginx", "/var/www", "/etc/letsencrypt"]
extra_paths = ["/var/backups/ghostsnap/web-01"]
tags = ["host:web-01", "service:nginx", "type:website", "target:b2"]
exclude = ["*/cache/*", "*.tmp", "node_modules"]

pre_hook = """
mkdir -p /var/backups/ghostsnap/web-01
mysqldump --defaults-extra-file=/root/.my.cnf --single-transaction appdb \
  > /var/backups/ghostsnap/web-01/appdb.sql
"""

post_hook = "rm -rf /var/backups/ghostsnap/web-01"

keep_daily = 7
keep_weekly = 4
keep_monthly = 6
prune = true
```

**Required environment for B2:**
```bash
export AWS_ACCESS_KEY_ID="your-b2-key-id"
export AWS_SECRET_ACCESS_KEY="your-b2-application-key"
export AWS_ENDPOINT_URL="https://s3.us-west-004.backblazeb2.com"
```

### Example: Website to Azure Blob

```toml
version = 1

[defaults]
password_file = "/etc/ghostsnap/password"

[jobs.website-azure]
repository = "azure:mystorageaccount/website-backups/web-01"
paths = ["/etc/nginx", "/var/www", "/etc/letsencrypt"]
extra_paths = ["/var/backups/ghostsnap/web-01"]
tags = ["host:web-01", "service:nginx", "type:website", "target:azure"]

pre_hook = """
mkdir -p /var/backups/ghostsnap/web-01
pg_dump -Fc myapp > /var/backups/ghostsnap/web-01/myapp.dump
"""

post_hook = "rm -rf /var/backups/ghostsnap/web-01"

keep_daily = 7
keep_weekly = 4
prune = true
```

**Required environment for Azure:**
```bash
export AZURE_STORAGE_ACCOUNT="mystorageaccount"
export AZURE_STORAGE_KEY="your-storage-key"
```

## Running Backups

### Validate Configuration

Before running your first backup, validate the job:

```bash
ghostsnap job --config /etc/ghostsnap/jobs.toml validate website-b2
```

### Run Backup Manually

```bash
ghostsnap job --config /etc/ghostsnap/jobs.toml run website-b2
```

### Dry Run

Test without making changes:

```bash
ghostsnap job --config /etc/ghostsnap/jobs.toml run website-b2 --dry-run
```

## Restore Walkthroughs

### Understanding Snapshot Paths

Ghostsnap stores files with paths **relative to each backup source**. When you back up `/etc/nginx`, `/var/www`, and `/etc/letsencrypt`:

- Files from `/etc/nginx/nginx.conf` are stored as `nginx.conf`
- Files from `/var/www/html/index.html` are stored as `html/index.html`
- Files from `/etc/letsencrypt/live/...` are stored as `live/...`

Use `ghostsnap ls` to see the actual paths stored in a snapshot:

```bash
# List top-level contents
ghostsnap --repo s3:my-b2-bucket/web-01 ls <snapshot-id>

# List all files recursively
ghostsnap --repo s3:my-b2-bucket/web-01 ls <snapshot-id> -r
```

### List Available Snapshots

```bash
ghostsnap --repo s3:my-b2-bucket/web-01 snapshots
```

### Full Site Restore

Restore to a staging directory first:

```bash
ghostsnap --repo s3:my-b2-bucket/web-01 restore <snapshot-id> --target /restore/staging
```

This creates the directory structure under `/restore/staging/` with all backed-up files.

Then inspect and copy to live locations:

```bash
# See what was restored
ls -la /restore/staging/

# Copy nginx configs (stored relative to /etc/nginx)
sudo cp -a /restore/staging/nginx.conf /etc/nginx/
sudo cp -a /restore/staging/sites-available /etc/nginx/
sudo nginx -t && sudo systemctl reload nginx

# Copy web content (stored relative to /var/www)
sudo cp -a /restore/staging/html /var/www/
```

### Restore Specific Paths

Restore only Nginx config files:

```bash
# First, list what's in the snapshot
ghostsnap --repo s3:my-b2-bucket/web-01 ls <snapshot-id>

# Restore files matching a path prefix
ghostsnap --repo s3:my-b2-bucket/web-01 restore <snapshot-id> \
  --target /restore/nginx \
  nginx.conf sites-available sites-enabled
```

### Restore Database Dump

Extract the database dump using `dump`:

```bash
# List files to find the dump path
ghostsnap --repo s3:my-b2-bucket/web-01 ls <snapshot-id> -r | grep sql

# Dump stores files relative to backup source, so use relative path
ghostsnap --repo s3:my-b2-bucket/web-01 dump <snapshot-id> \
  appdb.sql > /tmp/appdb.sql
```

Then import:

```bash
mysql appdb < /tmp/appdb.sql
```

### Restore Certificates

Be careful with certificate permissions:

```bash
# Restore cert files (stored relative to /etc/letsencrypt)
ghostsnap --repo s3:my-b2-bucket/web-01 restore <snapshot-id> \
  --target /restore/certs \
  live archive renewal

# Verify permissions
ls -la /restore/certs/live/

# If correct, replace live certs
sudo rm -rf /etc/letsencrypt.bak
sudo mv /etc/letsencrypt /etc/letsencrypt.bak
sudo mkdir /etc/letsencrypt
sudo cp -a /restore/certs/* /etc/letsencrypt/
sudo systemctl reload nginx
```

### Dry Run

Test a restore without writing files:

```bash
ghostsnap --repo s3:my-b2-bucket/web-01 restore <snapshot-id> \
  --target /restore/test \
  --dry-run
```

## Best Practices

### Tagging Convention

Use consistent tags for filtering and organization:

| Tag | Example | Purpose |
|-----|---------|---------|
| `host:` | `host:web-01` | Identify source host |
| `service:` | `service:nginx` | Service type |
| `type:` | `type:website` | Backup category |
| `env:` | `env:prod` | Environment |
| `target:` | `target:b2` | Storage target |

### Verify Backups

Run periodic checks:

```bash
ghostsnap --repo s3:my-b2-bucket/web-01 check
```

For thorough verification:

```bash
ghostsnap --repo s3:my-b2-bucket/web-01 check --read-data
```

### Test Restores

Periodically restore to a test location to verify backup integrity:

```bash
ghostsnap --repo s3:my-b2-bucket/web-01 restore latest \
  --target /tmp/restore-test \
  --dry-run
```

### Exclude Patterns

Common excludes for website backups:

```toml
exclude = [
  "*/cache/*",
  "*/tmp/*",
  "*.tmp",
  "*.log",
  "node_modules",
  ".git",
  "__pycache__"
]
```

## Troubleshooting

### Pre-hook Fails

If the database dump fails:

1. Check the hook command manually:
   ```bash
   mysqldump --defaults-extra-file=/root/.my.cnf --single-transaction appdb > /tmp/test.sql
   ```

2. Verify credentials file exists and has correct permissions:
   ```bash
   ls -la /root/.my.cnf
   ```

3. Check disk space for staging directory:
   ```bash
   df -h /var/backups
   ```

### Restore Permission Denied

Run with sudo for system files:

```bash
sudo ghostsnap --repo /backup/repo restore <id> --target /restore
```

Or restore without ownership:

```bash
ghostsnap --repo /backup/repo restore <id> --target /restore --no-ownership
```

### Snapshot Not Found

List available snapshots with filters:

```bash
ghostsnap --repo s3:my-b2-bucket/web-01 snapshots --tag host:web-01
```

Use short snapshot IDs (first 8 characters):

```bash
ghostsnap --repo s3:my-b2-bucket/web-01 restore a1b2c3d4 --target /restore
```
