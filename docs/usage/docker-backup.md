# Docker Backup Workflows

This guide covers backup strategies for Docker containers and their data using Ghostsnap.

## Overview

Docker data comes in several forms, each requiring different backup approaches:

1. **Bind mounts** - Host directories mounted into containers
2. **Named volumes** - Docker-managed persistent storage
3. **Container configuration** - Docker Compose files, environment variables
4. **Database state** - Application databases requiring consistent snapshots

## Backing Up Bind Mounts

Bind mounts are directories on the host filesystem. Back them up directly.

```yaml
# docker-compose.yml
services:
  app:
    volumes:
      - /data/app-data:/app/data
      - ./config:/app/config
```

```bash
# Back up the bind mount paths directly
ghostsnap --repo /backup/repo backup /data/app-data --tag app-data
ghostsnap --repo /backup/repo backup ./config --tag app-config
```

## Backing Up Named Volumes

Named volumes are managed by Docker and stored in `/var/lib/docker/volumes/`. Access them by mounting temporarily or using `docker cp`.

### Method 1: Direct Path Backup (Requires Root)

```bash
# Find volume location
docker volume inspect myapp_data --format '{{ .Mountpoint }}'
# Output: /var/lib/docker/volumes/myapp_data/_data

# Back up as root
sudo ghostsnap --repo /backup/repo backup /var/lib/docker/volumes/myapp_data/_data --tag myapp-data
```

### Method 2: Mount and Backup via Container

```bash
# Create a temporary container to access volume data
docker run --rm -v myapp_data:/data -v /tmp/backup:/backup alpine \
  tar -czf /backup/myapp-data.tar.gz -C /data .

# Back up the tarball
ghostsnap --repo /backup/repo backup /tmp/backup/myapp-data.tar.gz --tag myapp-data
rm /tmp/backup/myapp-data.tar.gz
```

### Method 3: Live Container Volume Backup

```bash
# Copy data out of running container
docker cp myapp:/app/data /tmp/backup/myapp-data

# Back up the copied data
ghostsnap --repo /backup/repo backup /tmp/backup/myapp-data --tag myapp-data
rm -rf /tmp/backup/myapp-data
```

## Pre-Backup Database Dumps

For databases, always create a consistent dump before backing up files.

### MySQL/MariaDB

```bash
#!/bin/bash
# mysql-backup.sh

DUMP_DIR="/tmp/db-dumps"
mkdir -p "$DUMP_DIR"

# Dump database (inside container)
docker exec mysql-container mysqldump \
  -u root -p"${MYSQL_ROOT_PASSWORD}" \
  --all-databases --single-transaction \
  > "$DUMP_DIR/mysql-all.sql"

# Back up dump file
ghostsnap --repo /backup/repo backup "$DUMP_DIR" --tag mysql-dump

# Clean up
rm -rf "$DUMP_DIR"
```

### PostgreSQL

```bash
#!/bin/bash
# postgres-backup.sh

DUMP_DIR="/tmp/db-dumps"
mkdir -p "$DUMP_DIR"

# Dump all databases
docker exec postgres-container pg_dumpall \
  -U postgres \
  > "$DUMP_DIR/postgres-all.sql"

# Back up dump file
ghostsnap --repo /backup/repo backup "$DUMP_DIR" --tag postgres-dump

# Clean up
rm -rf "$DUMP_DIR"
```

### MongoDB

```bash
#!/bin/bash
# mongo-backup.sh

DUMP_DIR="/tmp/db-dumps"

# Dump using mongodump
docker exec mongo-container mongodump \
  --out /tmp/mongodump

# Copy out of container
docker cp mongo-container:/tmp/mongodump "$DUMP_DIR"

# Back up
ghostsnap --repo /backup/repo backup "$DUMP_DIR" --tag mongo-dump

# Clean up
rm -rf "$DUMP_DIR"
docker exec mongo-container rm -rf /tmp/mongodump
```

## Complete Stack Backup Script

Example backup script for a typical web application stack:

```bash
#!/bin/bash
# backup-stack.sh
set -e

REPO="/backup/repo"
DUMP_DIR="/tmp/backup-staging"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)

mkdir -p "$DUMP_DIR"

echo "=== Starting stack backup: $TIMESTAMP ==="

# 1. Stop non-critical containers (optional, for consistency)
# docker-compose stop app

# 2. Database dump
echo "Dumping database..."
docker exec db mysqldump -u root -p"$DB_PASSWORD" \
  --all-databases --single-transaction \
  > "$DUMP_DIR/database.sql"

# 3. Back up database dump
echo "Backing up database dump..."
ghostsnap --repo "$REPO" backup "$DUMP_DIR" --tag "db-$TIMESTAMP"

# 4. Back up application data (bind mounts)
echo "Backing up application data..."
ghostsnap --repo "$REPO" backup /data/app-uploads --tag "uploads-$TIMESTAMP"
ghostsnap --repo "$REPO" backup /data/app-config --tag "config-$TIMESTAMP"

# 5. Back up Docker Compose configuration
echo "Backing up container configuration..."
ghostsnap --repo "$REPO" backup /opt/myapp/docker-compose.yml --tag "compose-$TIMESTAMP"
ghostsnap --repo "$REPO" backup /opt/myapp/.env --tag "env-$TIMESTAMP"

# 6. Restart containers if stopped
# docker-compose start app

# 7. Clean up
rm -rf "$DUMP_DIR"

echo "=== Backup complete: $TIMESTAMP ==="

# 8. Apply retention policy
ghostsnap --repo "$REPO" forget --keep-daily 7 --keep-weekly 4 --keep-monthly 6
ghostsnap --repo "$REPO" prune
```

## WordPress Docker Backup

Example for WordPress with MySQL:

```bash
#!/bin/bash
# wordpress-backup.sh

REPO="/backup/repo"
DUMP_DIR="/tmp/wp-backup"

mkdir -p "$DUMP_DIR"

# Database dump
docker exec wordpress-db mysqldump \
  -u wordpress -p"$WORDPRESS_DB_PASSWORD" \
  wordpress > "$DUMP_DIR/wordpress.sql"

# Back up database
ghostsnap --repo "$REPO" backup "$DUMP_DIR" --tag wordpress-db

# Back up WordPress files (uploads, plugins, themes)
ghostsnap --repo "$REPO" backup /var/www/html/wp-content --tag wordpress-content

# Clean up
rm -rf "$DUMP_DIR"
```

## Restore Workflow

### Restore Database

```bash
# Restore database dump
ghostsnap --repo /backup/repo restore <snapshot-id> --target /tmp/restore

# Import into database
docker exec -i mysql-container mysql -u root -p"$PASSWORD" < /tmp/restore/database.sql
```

### Restore Application Data

```bash
# Restore bind mount data
ghostsnap --repo /backup/repo restore <snapshot-id> --target /data/app-uploads

# Restore named volume
ghostsnap --repo /backup/repo restore <snapshot-id> --target /tmp/restore
docker run --rm -v myapp_data:/data -v /tmp/restore:/backup alpine \
  cp -a /backup/. /data/
```

## Scheduling with Cron

```cron
# Run backup daily at 2 AM
0 2 * * * /opt/scripts/backup-stack.sh >> /var/log/backup.log 2>&1
```

## Best Practices

1. **Always dump databases first** - File-based backups of running databases can be inconsistent
2. **Test restores regularly** - Backups are worthless if you can't restore from them
3. **Use tags** - Tag snapshots with date/purpose for easy identification
4. **Apply retention policies** - Use `forget` and `prune` to manage storage
5. **Back up configuration** - Include docker-compose.yml and .env files
6. **Consider downtime** - For critical consistency, stop containers during backup
7. **Use offsite storage** - Copy important backups to S3, Azure, or rclone targets

```bash
# Copy critical backups offsite
ghostsnap --repo /backup/repo copy --repo2 s3:bucket/offsite <snapshot-id>
```
