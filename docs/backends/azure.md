# Azure Blob Storage Backend

Ghostsnap supports Azure Blob Storage as a repository target for secure, scalable cloud backups.

## Quick Start

```bash
# Set credentials
export AZURE_STORAGE_KEY="your-storage-account-key"

# Initialize repository
ghostsnap init --backend azure --account-name mystorageaccount --container backups

# Back up data
ghostsnap --repo azure:mystorageaccount/backups backup /data --tag daily

# List snapshots
ghostsnap --repo azure:mystorageaccount/backups snapshots

# Restore
ghostsnap --repo azure:mystorageaccount/backups restore abc123 --target /restore
```

## URI Format

```
azure:<account-name>/<container>
azure:<account-name>/<container>/<prefix>
```

Examples:
- `azure:mystorageaccount/backups` - Container root
- `azure:mystorageaccount/backups/ghostsnap` - With prefix

## Authentication

### Storage Account Key (Recommended)

Set the storage account key via environment variable:

```bash
export AZURE_STORAGE_KEY="your-storage-account-key"
# or
export AZURE_STORAGE_ACCESS_KEY="your-storage-account-key"
```

Find your key in Azure Portal: Storage Account > Access keys

### Azure CLI Integration

Ghostsnap works alongside the Azure CLI. If you use `az` for account management, Ghostsnap uses the storage key for data operations:

```bash
# Login with Azure CLI (for account management)
az login

# List storage accounts
az storage account list --output table

# Get storage account key
az storage account keys list --account-name mystorageaccount --query '[0].value' -o tsv

# Set for Ghostsnap
export AZURE_STORAGE_KEY=$(az storage account keys list --account-name mystorageaccount --query '[0].value' -o tsv)
```

## Initialize Repository

### Basic Initialization

```bash
ghostsnap init --backend azure \
  --account-name mystorageaccount \
  --container backups
```

### With Prefix

Use a prefix to organize multiple repositories in one container:

```bash
ghostsnap init --backend azure \
  --account-name mystorageaccount \
  --container backups \
  --azure-prefix production
```

### Using URI Format

```bash
ghostsnap init azure:mystorageaccount/backups/production
```

## Backup Operations

```bash
# Simple backup
ghostsnap --repo azure:mystorageaccount/backups backup /home/user/documents

# With tags
ghostsnap --repo azure:mystorageaccount/backups backup /data --tag server1 --tag daily

# Exclude patterns
ghostsnap --repo azure:mystorageaccount/backups backup /data --exclude "*.tmp" --exclude ".cache"
```

## Restore Operations

```bash
# List snapshots
ghostsnap --repo azure:mystorageaccount/backups snapshots

# Restore to target directory
ghostsnap --repo azure:mystorageaccount/backups restore abc123 --target /restore

# Restore specific path
ghostsnap --repo azure:mystorageaccount/backups restore abc123 --include "documents/**" --target /restore
```

## Repository Maintenance

```bash
# Check integrity
ghostsnap --repo azure:mystorageaccount/backups check

# Show statistics
ghostsnap --repo azure:mystorageaccount/backups stats

# Apply retention policy
ghostsnap --repo azure:mystorageaccount/backups forget --keep-daily 7 --keep-weekly 4

# Remove unreferenced data
ghostsnap --repo azure:mystorageaccount/backups prune
```

## Copy Between Repositories

Copy snapshots from local to Azure:

```bash
ghostsnap --repo /local/backup copy --repo2 azure:mystorageaccount/backups abc123
```

Copy from Azure to local:

```bash
ghostsnap --repo azure:mystorageaccount/backups copy --repo2 /local/backup abc123
```

## Best Practices

### Container Setup

1. Create a dedicated container for backups
2. Use a consistent naming convention
3. Consider geo-redundant storage (GRS) for critical data

```bash
# Create container via Azure CLI
az storage container create --name backups --account-name mystorageaccount
```

### Security

1. Use a dedicated storage account for backups
2. Enable soft delete for blob protection
3. Consider immutable storage for compliance
4. Rotate storage keys periodically
5. Use managed identities in Azure VMs (future support)

### Cost Optimization

1. Use Cool or Archive tiers for long-term retention
2. Enable lifecycle management policies
3. Monitor storage consumption with `ghostsnap stats`

## Troubleshooting

### Authentication Failed

```
Error: Azure authentication failed
```

Check that AZURE_STORAGE_KEY is set correctly:

```bash
echo $AZURE_STORAGE_KEY | head -c 10
```

Verify the key works with Azure CLI:

```bash
az storage container list --account-name mystorageaccount --account-key $AZURE_STORAGE_KEY
```

### Container Not Found

```
Error: Container 'backups' not found
```

Create the container first:

```bash
az storage container create --name backups --account-name mystorageaccount
```

### Network Errors

For intermittent network issues, Ghostsnap automatically retries operations. If problems persist:

1. Check network connectivity to Azure
2. Verify firewall rules allow outbound HTTPS
3. Check Azure service status

## Environment Variables

| Variable | Description |
|----------|-------------|
| `AZURE_STORAGE_KEY` | Storage account access key |
| `AZURE_STORAGE_ACCESS_KEY` | Alternative name for storage key |
| `GHOSTSNAP_REPO` | Default repository path |
| `GHOSTSNAP_PASSWORD` | Repository password |
