# Configuration

Ghostsnap is currently configured via environment variables and command-line flags.

There is no active `config.toml` loader in the current CLI, so any configuration-file examples should be treated as future design notes rather than working behavior.

## Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `GHOSTSNAP_REPO` | Default repository path | `/backup/repo` |
| `GHOSTSNAP_PASSWORD` | Repository password | `mysecretpassword` |
| `GHOSTSNAP_PASSWORD_FILE` | File containing password | `/etc/ghostsnap/password` |
| `AWS_ACCESS_KEY_ID` | S3 access key | `AKIAIOSFODNN7EXAMPLE` |
| `AWS_SECRET_ACCESS_KEY` | S3 secret key | `wJalrXUtnFEMI/K7MDENG/...` |
| `AWS_REGION` | S3 region | `us-west-2` |
| `AWS_ENDPOINT_URL` | Optional S3-compatible endpoint | `https://s3.us-west-1.wasabisys.com` |
| `AZURE_STORAGE_ACCOUNT` | Azure account name | `myaccount` |
| `AZURE_STORAGE_KEY` | Azure access key | `base64key...` |
| `B2_APPLICATION_KEY_ID` | Backblaze key ID | `000abc123...` |
| `B2_APPLICATION_KEY` | Backblaze app key | `K000abc...` |

## Command-Line Flags

Global flags available for all commands:

```bash
ghostsnap [OPTIONS] <COMMAND>

Options:
  -r, --repo <REPO>        Repository location
  -p, --password <PASS>    Repository password
  -v, --verbose            Verbose output
  -q, --quiet              Suppress non-error output
  -h, --help               Print help
  -V, --version            Print version
```

## S3 Provider Notes

Native S3 repository support should work with AWS S3 and can often work with S3-compatible providers when an endpoint override is supplied.

Practical targets to validate explicitly before relying on them in production:

- AWS S3
- Wasabi
- Backblaze B2 S3-compatible API
- Cloudflare R2
- any custom S3-compatible endpoint you provide with `--endpoint`

These providers should be treated as compatibility targets to test, not as guaranteed first-class integrations unless you have executed the release validation matrix against them.

## Platform Notes

Ghostsnap is developed and tested primarily on Linux.

For an Arch Linux + Btrfs system, the important practical concerns are:

- snapshot source paths may include Btrfs subvolumes that should be selected intentionally
- restore targets should be validated against your expected mount/subvolume layout
- extended attributes, hardlinks, and sparse files should be validated on your actual filesystem before relying on them for disaster recovery
- if you want Btrfs snapshot orchestration itself, that is a separate feature from Ghostsnap's repository format and is not yet implemented

## Password Management

### Interactive Prompt

If no password is provided, Ghostsnap prompts interactively:

```bash
ghostsnap backup /data
Enter repository password: ********
```

### Environment Variable

```bash
export GHOSTSNAP_PASSWORD="mysecretpassword"
ghostsnap backup /data
```

### Password File

```bash
echo "mysecretpassword" > /etc/ghostsnap/password
chmod 600 /etc/ghostsnap/password
export GHOSTSNAP_PASSWORD_FILE=/etc/ghostsnap/password
ghostsnap backup /data
```

### Password Command (Advanced)

```bash
export GHOSTSNAP_PASSWORD_COMMAND="pass show ghostsnap"
ghostsnap backup /data
```

## Exclude Patterns

Exclude files using glob patterns:

```bash
# Command line
ghostsnap backup /data -e "*.log" -e "*.tmp" -e ".git"

# Exclude if marker file present
ghostsnap backup /data --exclude-if-present .nobackup
```

Common exclusions:

```bash
# Development files
-e "node_modules" -e "__pycache__" -e ".git" -e "target"

# Temporary files
-e "*.tmp" -e "*.swp" -e "*.cache" -e ".DS_Store"

# Large media (if not needed)
-e "*.iso" -e "*.vmdk" -e "*.qcow2"
```

## Scheduled Backups

### Systemd Timer

Create `/etc/systemd/system/ghostsnap-backup.service`:

```ini
[Unit]
Description=Ghostsnap Backup

[Service]
Type=oneshot
Environment="GHOSTSNAP_REPO=/backup/repo"
Environment="GHOSTSNAP_PASSWORD_FILE=/etc/ghostsnap/password"
ExecStart=/usr/local/bin/ghostsnap backup /home /etc
```

Create `/etc/systemd/system/ghostsnap-backup.timer`:

```ini
[Unit]
Description=Daily Ghostsnap Backup

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

Enable:

```bash
sudo systemctl enable --now ghostsnap-backup.timer
```

### Cron

```cron
# Daily backup at 2 AM
0 2 * * * GHOSTSNAP_REPO=/backup/repo GHOSTSNAP_PASSWORD_FILE=/etc/ghostsnap/password /usr/local/bin/ghostsnap backup /home /etc
```
