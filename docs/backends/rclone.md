# Rclone Backend

Ghostsnap supports rclone as a repository backend, providing access to 40+ cloud storage providers through a single unified interface.

## Prerequisites

1. **Install rclone**: Download from [rclone.org](https://rclone.org/downloads/) or install via your package manager
2. **Configure a remote**: Run `rclone config` to set up your cloud storage provider

## Supported Providers

Through rclone, Ghostsnap can store backups on:

- Google Drive
- Dropbox
- OneDrive
- Box
- pCloud
- SFTP servers
- FTP servers
- WebDAV
- Backblaze B2
- Wasabi
- DigitalOcean Spaces
- And 30+ more providers

See [rclone's full provider list](https://rclone.org/overview/) for all options.

## Usage

### Initialize a Repository

```bash
# Using URI format
ghostsnap init rclone:myremote/backups/ghostsnap

# Using explicit flags
ghostsnap init --backend rclone --remote myremote --rclone-path backups/ghostsnap
```

### Backup

```bash
ghostsnap --repo rclone:myremote/backups/ghostsnap backup /data
```

### Restore

```bash
ghostsnap --repo rclone:myremote/backups/ghostsnap restore --target /restore latest
```

### Other Operations

All standard commands work with rclone repositories:

```bash
# List snapshots
ghostsnap --repo rclone:myremote/backups/ghostsnap snapshots

# Check repository integrity
ghostsnap --repo rclone:myremote/backups/ghostsnap check

# View statistics
ghostsnap --repo rclone:myremote/backups/ghostsnap stats

# Prune old snapshots
ghostsnap --repo rclone:myremote/backups/ghostsnap forget --keep-last 7
ghostsnap --repo rclone:myremote/backups/ghostsnap prune
```

## URI Format

```
rclone:<remote>/<path>
```

- `<remote>`: The rclone remote name (as configured in `rclone config`)
- `<path>`: Optional path within the remote

Examples:
- `rclone:gdrive` - Root of Google Drive remote named "gdrive"
- `rclone:gdrive/backups` - "backups" folder in Google Drive
- `rclone:s3remote/bucket/prefix` - S3-compatible storage via rclone
- `rclone:sftp-server/home/user/backups` - SFTP server

## Configuration Tips

### Google Drive

```bash
rclone config
# Choose "n" for new remote
# Name: gdrive
# Choose "drive" for Google Drive
# Follow OAuth prompts
```

### SFTP

```bash
rclone config
# Choose "n" for new remote
# Name: myserver
# Choose "sftp"
# Enter host, user, and authentication method
```

### Environment Variables

Rclone reads credentials from environment variables. See [rclone environment variables](https://rclone.org/docs/#environment-variables) for provider-specific options.

## Performance Considerations

- Rclone adds a process call overhead compared to native backends
- For high-throughput scenarios, consider native S3 or Azure backends if available
- Use `--transfers` flag in rclone config for parallel uploads
- Local caching can improve repeated access patterns

## Troubleshooting

### "rclone: command not found"

Ensure rclone is installed and in your PATH:

```bash
which rclone
rclone version
```

### "Remote not found"

Verify your remote is configured:

```bash
rclone listremotes
rclone lsd myremote:
```

### Authentication Errors

Re-authenticate the remote:

```bash
rclone config reconnect myremote:
```
