# MinIO Backend

MinIO is supported via S3 compatibility mode. The `minio:` URI scheme maps
internally to Ghostsnap's S3 backend, with the endpoint resolved from the
`MINIO_ENDPOINT` environment variable.

The MinIO server project is archived upstream, but it still implements the S3
protocol, so this integration remains valid.

## Quick Start

```bash
# Set the MinIO endpoint and credentials
export MINIO_ENDPOINT="https://minio.example.com:9000"
export AWS_ACCESS_KEY_ID="your-minio-access-key"
export AWS_SECRET_ACCESS_KEY="your-minio-secret-key"

# Initialize repository (the scheme is inferred from the URI)
ghostsnap init minio:my-bucket/backups

# Back up data
ghostsnap --repo minio:my-bucket/backups backup /data

# List snapshots
ghostsnap --repo minio:my-bucket/backups snapshots

# Restore
ghostsnap --repo minio:my-bucket/backups restore abc123 --target /restore
```

The `minio://my-bucket/backups` form is also accepted.

## Setup

### 1. Create Access Credentials

In your MinIO server, create an access key and secret key for the user that
should own the backups.

### 2. Configure Environment

```bash
export MINIO_ENDPOINT="https://minio.example.com:9000"
export AWS_ACCESS_KEY_ID="your-minio-access-key"
export AWS_SECRET_ACCESS_KEY="your-minio-secret-key"
```

The endpoint is resolved from `MINIO_ENDPOINT`, falling back to
`AWS_ENDPOINT_URL`. The region defaults to `us-east-1` (MinIO ignores it, but
the S3 client requires a value). Credentials use the standard
`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` variables, set to your MinIO
access key and secret key.

## Usage

All standard Ghostsnap commands work with MinIO via the S3 protocol:

```bash
# Initialize
ghostsnap init minio:my-bucket/backups

# Backup
ghostsnap --repo minio:my-bucket/backups backup /data --tag daily

# Check integrity
ghostsnap --repo minio:my-bucket/backups check

# Stats
ghostsnap --repo minio:my-bucket/backups stats

# Forget old snapshots
ghostsnap --repo minio:my-bucket/backups forget --keep-daily 7 --keep-weekly 4

# Prune unused data
ghostsnap --repo minio:my-bucket/backups prune
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `MINIO_ENDPOINT` | MinIO server URL. Falls back to `AWS_ENDPOINT_URL`. |
| `AWS_ACCESS_KEY_ID` | MinIO access key. |
| `AWS_SECRET_ACCESS_KEY` | MinIO secret key. |

## Alternative: Explicit S3 Backend

The `minio:` scheme is a convenience over the S3 backend. The equivalent
explicit form also works:

```bash
ghostsnap init --backend s3 \
  --bucket my-bucket \
  --endpoint https://minio.example.com:9000 \
  s3:my-bucket/backups
```

## See Also

- [S3 Backend](s3.md) - Full S3 documentation
- [Backblaze B2](b2.md) - Another S3-compatible provider
- [Local Storage](local.md) - File system backend
