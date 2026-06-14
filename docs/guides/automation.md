# Backup Automation Guide

This guide covers automating Ghostsnap backups with systemd timers and cron.

## Systemd Timer (Recommended)

Systemd timers provide better logging, failure handling, and service management than cron.

### Service Unit

Create `/etc/systemd/system/ghostsnap-backup.service`:

```ini
[Unit]
Description=Ghostsnap Backup Job
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
User=root
ExecStart=/usr/local/bin/ghostsnap job --config /etc/ghostsnap/jobs.toml run --all

# Environment for cloud credentials
EnvironmentFile=-/etc/ghostsnap/environment

# Logging
StandardOutput=journal
StandardError=journal

# Timeouts
TimeoutStartSec=3600

[Install]
WantedBy=multi-user.target
```

### Timer Unit

Create `/etc/systemd/system/ghostsnap-backup.timer`:

```ini
[Unit]
Description=Run Ghostsnap backup nightly

[Timer]
OnCalendar=*-*-* 02:00:00
RandomizedDelaySec=900
Persistent=true

[Install]
WantedBy=timers.target
```

### Environment File

Create `/etc/ghostsnap/environment`:

```bash
# Backblaze B2
AWS_ACCESS_KEY_ID=your-key-id
AWS_SECRET_ACCESS_KEY=your-application-key
AWS_ENDPOINT_URL=https://s3.us-west-004.backblazeb2.com

# Or Azure
# AZURE_STORAGE_ACCOUNT=mystorageaccount
# AZURE_STORAGE_KEY=your-storage-key
```

Secure the file:

```bash
chmod 600 /etc/ghostsnap/environment
```

### Enable Timer

```bash
sudo systemctl daemon-reload
sudo systemctl enable ghostsnap-backup.timer
sudo systemctl start ghostsnap-backup.timer
```

### Check Status

```bash
# Timer status
systemctl status ghostsnap-backup.timer

# Next run time
systemctl list-timers ghostsnap-backup.timer

# Last run logs
journalctl -u ghostsnap-backup.service -n 50
```

### Manual Run

```bash
sudo systemctl start ghostsnap-backup.service
```

## Cron

For simpler setups or systems without systemd.

### Basic Cron Job

Edit crontab:

```bash
sudo crontab -e
```

Add nightly backup at 2:00 AM:

```bash
0 2 * * * /usr/local/bin/ghostsnap job --config /etc/ghostsnap/jobs.toml run --all >> /var/log/ghostsnap-backup.log 2>&1
```

### With Environment Variables

Create a wrapper script `/usr/local/bin/ghostsnap-backup.sh`:

```bash
#!/bin/bash
set -euo pipefail

# Load credentials
source /etc/ghostsnap/environment

# Run backup
/usr/local/bin/ghostsnap job --config /etc/ghostsnap/jobs.toml run --all

# Optional: notify on completion
# curl -s https://healthchecks.io/ping/your-uuid
```

Make executable:

```bash
chmod 755 /usr/local/bin/ghostsnap-backup.sh
```

Add to crontab:

```bash
0 2 * * * /usr/local/bin/ghostsnap-backup.sh >> /var/log/ghostsnap-backup.log 2>&1
```

### Log Rotation

Create `/etc/logrotate.d/ghostsnap`:

```
/var/log/ghostsnap-backup.log {
    weekly
    rotate 4
    compress
    missingok
    notifempty
}
```

## Credential Management

### Password File

Store the repository password securely:

```bash
# Create password file
echo "your-secure-password" > /etc/ghostsnap/password
chmod 600 /etc/ghostsnap/password
```

Reference in job config:

```toml
[defaults]
password_file = "/etc/ghostsnap/password"
```

### Environment Variables

For cloud provider credentials, use environment files:

```bash
# /etc/ghostsnap/environment
GHOSTSNAP_PASSWORD=your-repo-password
AWS_ACCESS_KEY_ID=your-key-id
AWS_SECRET_ACCESS_KEY=your-secret-key
```

Secure the file:

```bash
chmod 600 /etc/ghostsnap/environment
chown root:root /etc/ghostsnap/environment
```

## Failure Notifications

### Systemd OnFailure

Add failure notification to the service unit:

```ini
[Unit]
Description=Ghostsnap Backup Job
OnFailure=ghostsnap-backup-failed@%n.service
```

Create notification service `/etc/systemd/system/ghostsnap-backup-failed@.service`:

```ini
[Unit]
Description=Ghostsnap backup failure notification

[Service]
Type=oneshot
ExecStart=/usr/local/bin/notify-backup-failed.sh %i
```

### Email Notification Script

Create `/usr/local/bin/notify-backup-failed.sh`:

```bash
#!/bin/bash
SERVICE=$1
HOST=$(hostname)
DATE=$(date)

echo "Backup failed on $HOST at $DATE. Check: journalctl -u $SERVICE" | \
  mail -s "Backup Failed: $HOST" admin@example.com
```

### Healthchecks.io Integration

Add to your backup wrapper script:

```bash
#!/bin/bash
set -euo pipefail

HEALTHCHECK_URL="https://hc-ping.com/your-uuid"

# Signal start
curl -fsS --retry 3 "${HEALTHCHECK_URL}/start" > /dev/null

# Run backup
if /usr/local/bin/ghostsnap job --config /etc/ghostsnap/jobs.toml run --all; then
    # Signal success
    curl -fsS --retry 3 "${HEALTHCHECK_URL}" > /dev/null
else
    # Signal failure
    curl -fsS --retry 3 "${HEALTHCHECK_URL}/fail" > /dev/null
    exit 1
fi
```

## Multiple Job Scheduling

### Run Specific Jobs at Different Times

Create multiple timer units:

**Website backup (2 AM):**
```ini
# /etc/systemd/system/ghostsnap-website.timer
[Timer]
OnCalendar=*-*-* 02:00:00
```

**Database backup (every 6 hours):**
```ini
# /etc/systemd/system/ghostsnap-database.timer
[Timer]
OnCalendar=*-*-* 00,06,12,18:00:00
```

Create corresponding service units that run specific jobs:

```ini
ExecStart=/usr/local/bin/ghostsnap job --config /etc/ghostsnap/jobs.toml run website-b2
```

### Staggered Execution

Use `RandomizedDelaySec` to avoid thundering herd:

```ini
[Timer]
OnCalendar=*-*-* 02:00:00
RandomizedDelaySec=1800  # Up to 30 minutes random delay
```

## Monitoring

### Check Last Backup Time

```bash
ghostsnap --repo /backup/repo snapshots --latest 1
```

### Alert on Stale Backups

Create a monitoring script:

```bash
#!/bin/bash
MAX_AGE_HOURS=26  # Alert if no backup in 26 hours

LAST_BACKUP=$(ghostsnap --repo /backup/repo snapshots --json | \
  jq -r '.[0].time' | xargs -I{} date -d {} +%s)
NOW=$(date +%s)
AGE_HOURS=$(( (NOW - LAST_BACKUP) / 3600 ))

if [ "$AGE_HOURS" -gt "$MAX_AGE_HOURS" ]; then
    echo "ALERT: Last backup was $AGE_HOURS hours ago"
    exit 1
fi
```

### Prometheus Metrics

For more advanced monitoring, wrap Ghostsnap execution and emit metrics:

```bash
#!/bin/bash
START=$(date +%s)

if ghostsnap job --config /etc/ghostsnap/jobs.toml run --all; then
    STATUS=1
else
    STATUS=0
fi

END=$(date +%s)
DURATION=$((END - START))

# Write to textfile for node_exporter
cat > /var/lib/prometheus/ghostsnap.prom << EOF
# HELP ghostsnap_backup_success Whether the last backup succeeded
# TYPE ghostsnap_backup_success gauge
ghostsnap_backup_success $STATUS
# HELP ghostsnap_backup_duration_seconds Duration of the last backup
# TYPE ghostsnap_backup_duration_seconds gauge
ghostsnap_backup_duration_seconds $DURATION
# HELP ghostsnap_backup_last_timestamp_seconds Timestamp of the last backup
# TYPE ghostsnap_backup_last_timestamp_seconds gauge
ghostsnap_backup_last_timestamp_seconds $END
EOF
```

## Best Practices

1. **Test your automation** before relying on it:
   ```bash
   sudo systemctl start ghostsnap-backup.service
   journalctl -u ghostsnap-backup.service -f
   ```

2. **Use `Persistent=true`** in timers so missed runs execute on boot

3. **Set reasonable timeouts** in service units

4. **Monitor for failures** - don't assume backups are running

5. **Verify backups periodically** with `ghostsnap check`

6. **Test restores** regularly to catch issues early

7. **Secure credentials** with proper file permissions (600)

8. **Use separate jobs** for different retention requirements
