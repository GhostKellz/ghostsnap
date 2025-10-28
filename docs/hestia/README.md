# HestiaCP Integration

**Status**: Phase 1 Complete ✅  
**Version**: 0.1.0  
**Last Updated**: October 2, 2025

---

## 📖 Overview

Ghostsnap provides native integration with [HestiaCP](https://hestiacp.com/), a powerful open-source web hosting control panel. This integration allows you to backup and restore HestiaCP users, domains, databases, and configurations using Ghostsnap's production-ready storage backends.

### Key Features

- ✅ **Wrapper-based Integration** - Leverages HestiaCP's native backup system
- ✅ **Multi-User Support** - Backup single user or all users at once
- ✅ **Pattern Matching** - Include/exclude users with glob patterns
- ✅ **Automatic Cleanup** - Manage local tarball retention
- ✅ **Progress Tracking** - Real-time feedback during operations
- ✅ **Multiple Backends** - Store backups in MinIO, S3, Azure, or local storage
- ✅ **Restore Support** - Restore users from Ghostsnap repository
- ✅ **User Discovery** - List and inspect HestiaCP users

---

## 🚀 Quick Start

### Prerequisites

- HestiaCP installed (`/usr/local/hestia`)
- Ghostsnap installed and configured
- Root or sudo access (for HestiaCP commands)
- Ghostsnap repository initialized

### Basic Usage

```bash
# 1. List HestiaCP users
ghostsnap hestia list-users

# 2. Backup a single user
ghostsnap hestia backup \
  --user admin \
  --repository /var/ghostsnap/repo \
  --cleanup

# 3. Backup all users
ghostsnap hestia backup \
  --repository /var/ghostsnap/repo \
  --cleanup

# 4. Restore a user
ghostsnap hestia restore admin \
  --snapshot hestia-admin-20251002-140000 \
  --repository /var/ghostsnap/repo
```

---

## 📚 Documentation Index

### Getting Started
- **[Installation Guide](installation.md)** - Set up HestiaCP integration
- **[Quick Start](quickstart.md)** - Get started in 5 minutes
- **[Configuration](configuration.md)** - Configure repositories and backends

### CLI Reference
- **[Commands Overview](commands/README.md)** - All available commands
- **[backup](commands/backup.md)** - Backup HestiaCP users
- **[restore](commands/restore.md)** - Restore from backups
- **[list-users](commands/list-users.md)** - List HestiaCP users
- **[list-backups](commands/list-backups.md)** - List backups in repository
- **[user-info](commands/user-info.md)** - Show user information

### Use Cases
- **[Backup Strategies](use-cases/backup-strategies.md)** - Best practices
- **[Disaster Recovery](use-cases/disaster-recovery.md)** - Recovery procedures
- **[Automation](use-cases/automation.md)** - Systemd and cron examples
- **[Multi-Destination](use-cases/multi-destination.md)** - 3-2-1 backup strategy

### Advanced
- **[Architecture](advanced/architecture.md)** - How it works
- **[Troubleshooting](advanced/troubleshooting.md)** - Common issues
- **[Performance](advanced/performance.md)** - Optimization tips
- **[Security](advanced/security.md)** - Security considerations

---

## 🎯 Use Case Examples

### Daily Backup to MinIO

```bash
#!/bin/bash
# Backup all HestiaCP users to MinIO daily

ghostsnap hestia backup \
  --repository /var/ghostsnap/minio \
  --cleanup \
  --keep-tarballs 3

# Result: All users backed up, only 3 most recent tarballs kept locally
```

### Selective Production Backup

```bash
# Only backup production users (prefix: prod-)
ghostsnap hestia backup \
  --repository /var/ghostsnap/prod \
  --include "prod-*" \
  --cleanup
```

### Disaster Recovery

```bash
# 1. List available backups
ghostsnap hestia list-backups \
  --repository /var/ghostsnap/minio \
  --user admin

# 2. Restore user
ghostsnap hestia restore admin \
  --snapshot hestia-admin-20251002-020000 \
  --repository /var/ghostsnap/minio

# 3. Apply to HestiaCP
v-restore-user admin /backup/restore-admin.tar
```

---

## 🔧 Architecture

### How It Works

```
┌─────────────────────┐
│   HestiaCP Server   │
│   v-backup-user     │
└──────────┬──────────┘
           │ Creates tarball
           ▼
    /backup/user.tar
           │
           │ Ghostsnap processes
           ▼
┌─────────────────────────┐
│  Ghostsnap Repository   │
│  • Chunk                │
│  • Encrypt (ChaCha20)   │
│  • Deduplicate (BLAKE3) │
└─────────────────────────┘
           │
           │ Upload
           ▼
   ┌───────┬────────┬────────┐
   │ MinIO │   S3   │ Azure  │
   └───────┴────────┴────────┘
```

### Components

1. **HestiaCP Native Backup** - Uses `v-backup-user` command
2. **Tarball Management** - Finds, tracks, and cleans up tarballs
3. **Repository Integration** - Chunks, encrypts, deduplicates
4. **Backend Storage** - Uploads to MinIO, S3, Azure, or local

---

## 🎨 Features

### Pattern Matching

Support for glob patterns to select users:

```bash
# Backup all production users
--include "prod-*"

# Exclude test users
--exclude "test-*"

# Combine patterns
--include "prod-*" --exclude "prod-test-*"
```

### Automatic Cleanup

Manage local disk space by controlling tarball retention:

```bash
# Keep last 3 tarballs (default)
--keep-tarballs 3

# Keep last 10 tarballs
--keep-tarballs 10

# Delete immediately after backup
--cleanup --keep-tarballs 0
```

### Progress Tracking

Real-time feedback during operations:

```
🚀 Starting backup for 5 user(s)

[1/5] Backing up user: admin ...
  📦 Creating HestiaCP backup...
  📊 Tarball size: 128.45 MB
  📁 Local tarball: /backup/admin.2025-10-02_14-00-00.tar
  ⬆️  Uploading to Ghostsnap repository...
  🔒 Encrypting and chunking...
  ☁️  Uploading chunks to backend...
  ✅ Backed up as snapshot: hestia-admin-20251002-140000
  🧹 Cleaned up 2 old tarball(s)
✅ Successfully backed up user: admin

🎉 Backup Summary:
   ✅ Successful: 5
   ❌ Failed: 0
```

---

## 🔐 Security

### Encryption

- All backups encrypted with **ChaCha20-Poly1305**
- Key derivation using **Argon2**
- Content-addressed storage with **BLAKE3** hashing

### Access Control

- Requires root/sudo for HestiaCP commands
- Repository password protection
- Backend credentials stored securely

### Data Integrity

- Checksums verified on upload/download
- Corruption detection
- Automatic retry on transient failures

---

## 📊 Performance

### Typical Performance

| Operation | Speed | Notes |
|-----------|-------|-------|
| HestiaCP Backup | 1-5 min/user | Depends on user size |
| Upload to MinIO | 50-100 MB/s | Local network |
| Upload to S3 | 20-50 MB/s | Internet speed dependent |
| Deduplication | 500+ MB/s | BLAKE3 hashing |
| Encryption | 400+ MB/s | ChaCha20 |

### Optimization Tips

1. **Use local MinIO** for fastest backups
2. **Enable cleanup** to save disk space
3. **Use pattern matching** to backup only needed users
4. **Schedule during off-hours** to reduce server load

---

## 🆘 Support

### Common Issues

- **"User not found"** - Check username spelling, run `hestia list-users`
- **"Repository not found"** - Initialize with `ghostsnap init`
- **"Permission denied"** - Run with sudo or as root
- **"Backup failed"** - Check disk space, HestiaCP logs

See [Troubleshooting Guide](advanced/troubleshooting.md) for more.

### Getting Help

- **Documentation**: This directory
- **GitHub Issues**: https://github.com/ghostkellz/ghostsnap/issues
- **HestiaCP Forums**: https://forum.hestiacp.com/

---

## 🗺️ Roadmap

### Phase 1 (Current - Complete ✅)
- ✅ Wrapper-based integration
- ✅ Single and multi-user backup
- ✅ Pattern matching
- ✅ Cleanup management
- ✅ User discovery

### Phase 2 (Planned)
- ⏳ Direct directory backup (skip tarballs)
- ⏳ Database dump integration
- ⏳ Selective restore (single domain/database)
- ⏳ Incremental backup support
- ⏳ Mail server backup

### Phase 3 (Future)
- 📋 Web UI
- 📋 Backup scheduling interface
- 📋 Monitoring dashboard
- 📋 Email notifications
- 📋 Backup health checks

---

## 📄 License

Ghostsnap is licensed under the MIT License. See [LICENSE](../../LICENSE) for details.

---

## 🙏 Acknowledgments

- **HestiaCP Team** - For the excellent control panel
- **Ghostsnap Contributors** - For building the core backup system

---

**Next**: Read the [Quick Start Guide](quickstart.md) to get started!
