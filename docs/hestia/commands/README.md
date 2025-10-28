# HestiaCP Commands Reference

Complete reference for all `ghostsnap hestia` commands.

---

## Command Overview

| Command | Description | Usage |
|---------|-------------|-------|
| [`backup`](backup.md) | Backup HestiaCP users | Daily backups, pattern matching |
| [`restore`](restore.md) | Restore from backups | Disaster recovery |
| [`list-users`](list-users.md) | List HestiaCP users | User discovery |
| [`list-backups`](list-backups.md) | List backups | Backup inventory |
| [`user-info`](user-info.md) | Show user details | User inspection |

---

## Global Options

These options apply to all `ghostsnap` commands:

```bash
--repo <PATH>              # Repository path (env: GHOSTSNAP_REPO)
--password <PASSWORD>      # Repository password (env: GHOSTSNAP_PASSWORD)
-v, --verbose              # Enable verbose output
-q, --quiet                # Enable quiet mode (errors only)
-h, --help                 # Print help information
```

---

## Common Patterns

### Backup Single User

```bash
ghostsnap hestia backup \
  --user admin \
  --repository /var/ghostsnap/repo \
  --cleanup
```

### Backup All Users

```bash
ghostsnap hestia backup \
  --repository /var/ghostsnap/repo \
  --cleanup
```

### Backup with Pattern Matching

```bash
# Include pattern
ghostsnap hestia backup \
  --repository /var/ghostsnap/repo \
  --include "prod-*" \
  --cleanup

# Exclude pattern
ghostsnap hestia backup \
  --repository /var/ghostsnap/repo \
  --exclude "test-*" \
  --cleanup
```

### List and Restore

```bash
# List backups
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/repo \
  --user admin

# Restore
ghostsnap hestia restore admin \
  --snapshot hestia-admin-20251002-140000 \
  --repository /var/ghostsnap/repo
```

---

## Command Details

### [`backup`](backup.md)

Backup HestiaCP user(s) to Ghostsnap repository.

**Quick Example**:
```bash
ghostsnap hestia backup --repository /var/ghostsnap/repo --cleanup
```

[→ Full documentation](backup.md)

---

### [`restore`](restore.md)

Restore HestiaCP user from Ghostsnap repository.

**Quick Example**:
```bash
ghostsnap hestia restore admin \
  --snapshot hestia-admin-20251002-140000 \
  --repository /var/ghostsnap/repo
```

[→ Full documentation](restore.md)

---

### [`list-users`](list-users.md)

List HestiaCP users available for backup.

**Quick Example**:
```bash
ghostsnap hestia list-users --detailed
```

[→ Full documentation](list-users.md)

---

### [`list-backups`](list-backups.md)

List backups in Ghostsnap repository.

**Quick Example**:
```bash
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/repo \
  --user admin
```

[→ Full documentation](list-backups.md)

---

### [`user-info`](user-info.md)

Show information about a HestiaCP user.

**Quick Example**:
```bash
ghostsnap hestia user-info admin
```

[→ Full documentation](user-info.md)

---

## Exit Codes

All commands follow standard Unix exit codes:

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error |
| `2` | Command-line usage error |
| `65` | Data error (corruption detected) |
| `74` | I/O error |
| `77` | Permission denied |

---

## Environment Variables

```bash
# Repository path
export GHOSTSNAP_REPO="/var/ghostsnap/hestia"

# Repository password
export GHOSTSNAP_PASSWORD="your-secure-password"

# Log level
export RUST_LOG="ghostsnap=info"
```

---

## Permissions

Most HestiaCP commands require **root or sudo** access:

```bash
# ✅ Correct
sudo ghostsnap hestia backup ...

# ❌ Will fail
ghostsnap hestia backup ...
# Error: Permission denied
```

**Exception**: `list-users` and `user-info` may work without sudo (read-only operations).

---

## Performance Considerations

### Parallel Backups

Currently, backups run sequentially. For faster multi-user backups:

```bash
# Backup users in batches
ghostsnap hestia backup --include "user[1-5]" --repository /var/ghostsnap/repo &
ghostsnap hestia backup --include "user[6-10]" --repository /var/ghostsnap/repo &
wait
```

### Disk Space Management

```bash
# Monitor disk space during backup
df -h /backup

# Clean up aggressively
ghostsnap hestia backup --cleanup --keep-tarballs 1

# Or delete immediately after upload
ghostsnap hestia backup --cleanup --keep-tarballs 0
```

---

## Examples by Use Case

### Daily Production Backup

```bash
#!/bin/bash
ghostsnap hestia backup \
  --repository /var/ghostsnap/production \
  --exclude "dev-*" \
  --exclude "test-*" \
  --cleanup \
  --keep-tarballs 3
```

### Weekly Full Backup

```bash
#!/bin/bash
# Run weekly, keep more tarballs
ghostsnap hestia backup \
  --repository /var/ghostsnap/weekly \
  --cleanup \
  --keep-tarballs 10
```

### Emergency Backup Before Maintenance

```bash
#!/bin/bash
# Backup all, keep tarballs for manual verification
ghostsnap hestia backup \
  --repository /var/ghostsnap/emergency \
  --keep-tarballs 999  # Don't auto-delete
```

---

## Next Steps

- **[Backup Strategies](../use-cases/backup-strategies.md)** - Best practices
- **[Automation Guide](../use-cases/automation.md)** - Systemd and cron
- **[Troubleshooting](../advanced/troubleshooting.md)** - Common issues

---

**Back to**: [HestiaCP Integration](../README.md)
