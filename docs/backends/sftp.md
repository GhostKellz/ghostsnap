# SFTP Backend

Ghostsnap supports storing repositories on any host reachable over SSH using the native SFTP backend. No external tools are required; the client speaks SSH/SFTP directly.

## Quick Start

```bash
# Initialize a repository over SFTP (the scheme is inferred from the URI)
ghostsnap init sftp:backup@nas.local:2222/srv/ghostsnap

# Back up data
ghostsnap --repo sftp:backup@nas.local:2222/srv/ghostsnap backup /data

# List snapshots
ghostsnap --repo sftp:backup@nas.local:2222/srv/ghostsnap snapshots

# Restore
ghostsnap --repo sftp:backup@nas.local:2222/srv/ghostsnap restore abc123 --target /restore
```

The `--backend sftp` flag is optional because the backend is inferred from the
`sftp:` URI scheme. It may still be passed explicitly:

```bash
ghostsnap init --backend sftp sftp:backup@nas.local:2222/srv/ghostsnap
```

## URI Format

The SFTP backend accepts `sftp:[user@]host[:port][/path]` or the equivalent
`sftp://...` form.

| Component | Required | Default | Notes |
|-----------|----------|---------|-------|
| `user`    | No       | `SFTP_USER`, else `USER` | Login user. Required overall: supply it in the URI or via `SFTP_USER`. |
| `host`    | Yes      | -       | Hostname or IP of the SSH server. |
| `port`    | No       | `22`    | SSH port. |
| `path`    | No       | -       | Remote directory for the repository. |

Examples:

```
sftp:nas.local/srv/ghostsnap
sftp:backup@nas.local/srv/ghostsnap
sftp:backup@nas.local:2222/srv/ghostsnap
sftp://backup@nas.local:2222/srv/ghostsnap
```

## Authentication

Authentication is resolved in the following order:

1. **Password** - If `SFTP_PASSWORD` is set, password authentication is used and
   takes precedence over key-based authentication.
2. **Private key** - The key specified by `SFTP_KEY_FILE`, if set. If that
   variable is unset, the default keys are searched in order:
   `~/.ssh/id_ed25519`, then `~/.ssh/id_ecdsa`.

If the private key is encrypted, provide its passphrase via
`SFTP_KEY_PASSPHRASE`.

> **Key types:** Ghostsnap's SFTP backend supports Ed25519 and ECDSA keys only,
> for both client authentication and host-key verification. RSA is not supported
> (see [docs/advisories](../advisories/) for the rationale).

```bash
# Password authentication
export SFTP_PASSWORD="your-password"
ghostsnap --repo sftp:backup@nas.local/srv/ghostsnap snapshots

# Explicit key file with passphrase
export SFTP_KEY_FILE="/home/me/.ssh/backup_ed25519"
export SFTP_KEY_PASSPHRASE="key-passphrase"
ghostsnap --repo sftp:backup@nas.local/srv/ghostsnap snapshots
```

## Host Key Verification

By default the server's host key is verified against `~/.ssh/known_hosts`. This
is secure by default: if the host is not present in `known_hosts`, the
connection is refused with guidance to add it.

Add the host key before the first connection:

```bash
ssh-keyscan -p 2222 nas.local >> ~/.ssh/known_hosts
```

To bypass host-key verification (not recommended), set:

```bash
export GHOSTSNAP_SFTP_INSECURE=1
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SFTP_USER` | Login user when not given in the URI. Falls back to `USER`. |
| `SFTP_PASSWORD` | Password for password authentication. Takes precedence when set. |
| `SFTP_KEY_FILE` | Path to an explicit private key. |
| `SFTP_KEY_PASSPHRASE` | Passphrase for an encrypted private key. |
| `GHOSTSNAP_SFTP_INSECURE` | Set to `1` to skip host-key verification (not recommended). |

## Usage

All standard Ghostsnap commands work over SFTP:

```bash
# Initialize
ghostsnap init sftp:backup@nas.local:2222/srv/ghostsnap

# Backup
ghostsnap --repo sftp:backup@nas.local:2222/srv/ghostsnap backup /data --tag daily

# List snapshots
ghostsnap --repo sftp:backup@nas.local:2222/srv/ghostsnap snapshots

# Restore
ghostsnap --repo sftp:backup@nas.local:2222/srv/ghostsnap restore <snapshot-id> --target /restore
```

## Limitations

Repository locking is not supported over SFTP. As with other remote backends, a
warning is logged and the operation proceeds without a lock. Avoid running
concurrent operations against the same remote repository.

## See Also

- [S3 Backend](s3.md) - S3 and S3-compatible storage
- [Local Storage](local.md) - File system backend
- [Rclone](rclone.md) - Additional cloud providers via rclone
