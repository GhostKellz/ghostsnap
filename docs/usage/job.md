# Job Command

The `job` command runs config-driven backups defined in a TOML file. It provides
a declarative way to describe repositories, paths, tags, excludes, hooks, and
retention so that backups can be run repeatably from cron, systemd, or by hand.

## Subcommands

```bash
ghostsnap job list                 # List all configured jobs
ghostsnap job show <name>          # Show resolved details of a job
ghostsnap job validate <name>      # Validate a job configuration
ghostsnap job run <name>           # Run a single job
ghostsnap job run --all            # Run every configured job
ghostsnap job run <name> --dry-run # Run without writing a backup
```

The `--config` / `-c` flag (or the `GHOSTSNAP_CONFIG` environment variable)
selects the configuration file:

```bash
ghostsnap job --config /etc/ghostsnap/jobs.toml run website-b2
```

`job run` accepts:

| Flag | Description |
|------|-------------|
| `<name>` | Name of the job to run. Required unless `--all` is used. |
| `--all` | Run all configured jobs. |
| `-n`, `--dry-run` | Walk and report without writing a backup. |

## Config File Locations

When `--config` is not supplied, the following locations are searched in order
and the first existing file is used:

1. The path in the `GHOSTSNAP_CONFIG` environment variable
2. `/etc/ghostsnap/jobs.toml`
3. `./ghostsnap.toml`

## Config Schema

A configuration file declares a format `version`, an optional `[defaults]`
table, and one `[jobs.<name>]` table per job.

```toml
version = 1

[defaults]
repository = "s3:my-bucket/backups"
password_env = "GHOSTSNAP_PASSWORD"

[jobs.nightly-web]
paths = ["/etc/nginx", "/var/www"]
tags = ["host:web-01", "service:nginx"]
keep_daily = 7
keep_weekly = 4
prune = true
```

`version` must be `1`.

### Defaults

Values in `[defaults]` apply to every job unless the job overrides them.

| Key | Type | Description |
|-----|------|-------------|
| `repository` | string | Default repository URI. |
| `password_env` | string | Environment variable holding the repository password. |
| `password_file` | path | File holding the repository password. |
| `shell` | string | Default shell for hooks. |

### Job Fields

Each `[jobs.<name>]` table accepts the following keys.

**Repository and credentials**

| Key | Type | Description |
|-----|------|-------------|
| `repository` | string | Repository URI. Overrides the default. |
| `password_env` | string | Environment variable holding the password. Overrides the default. |
| `password_file` | path | File holding the password. Overrides the default. |

A repository must be set on the job or in defaults. A password source
(`password_env` or `password_file`) is required; `password_env` is tried first,
then `password_file`.

**Sources and filtering**

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `paths` | list of paths | - | Paths to back up. |
| `extra_paths` | list of paths | `[]` | Additional paths, combined with `paths` (e.g. staging directories for dumps). |
| `tags` | list of strings | `[]` | Tags applied to the snapshot. |
| `exclude` | list of globs | `[]` | Glob patterns to exclude. Matched against the full path and the file/directory name. |
| `exclude_if_present` | list of strings | `[]` | Marker filenames; a directory containing one is skipped. |
| `hostname` | string | - | Override the hostname recorded in snapshot metadata. |
| `one_file_system` | bool | `false` | Do not cross mount points. |

**Hooks**

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `pre_hook` | string | - | Command run before the backup. |
| `post_hook` | string | - | Command run after the backup. Runs even if the backup fails. |
| `pre_hook_timeout` | duration | `5m` | Timeout for the pre-hook. |
| `post_hook_timeout` | duration | `5m` | Timeout for the post-hook. |
| `shell` | string | `/bin/sh` | Shell used to run hooks. |
| `working_directory` | path | - | Working directory for hooks. |

Durations are written as a number with an optional `s`, `m`, or `h` suffix
(e.g. `30s`, `5m`, `1h`). A bare number is interpreted as seconds.

**Retention**

| Key | Type | Description |
|-----|------|-------------|
| `keep_last` | integer | Keep the last N snapshots. |
| `keep_hourly` | integer | Keep N hourly snapshots. |
| `keep_daily` | integer | Keep N daily snapshots. |
| `keep_weekly` | integer | Keep N weekly snapshots. |
| `keep_monthly` | integer | Keep N monthly snapshots. |
| `keep_yearly` | integer | Keep N yearly snapshots. |
| `prune` | bool | Prune unreferenced data after applying retention. Default `false`. |

If no retention keys are set, all snapshots are kept and no forget step runs.

**Safety**

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `require_paths_exist` | bool | `true` | Fail the job if a configured path is missing. |
| `stop_on_pre_hook_failure` | bool | `true` | Abort the job if the pre-hook fails. |
| `dry_run` | bool | `false` | Walk and report without writing a backup. The `--dry-run` flag also enables this. |

## Execution Order

When a job runs, the steps execute in this order:

1. Resolve the password and validate paths (subject to `require_paths_exist`).
2. Run `pre_hook`. If it fails and `stop_on_pre_hook_failure` is true, the job
   aborts.
3. Open the repository. Local repositories acquire an exclusive lock; remote
   repositories log a warning that locking is not supported.
4. Perform the backup, creating a tagged snapshot.
5. Apply retention (`keep_*`) if any retention policy is configured.
6. Run `prune` if `prune = true`.
7. Run `post_hook`. It always runs, even if the backup failed.

## Example: Website to B2

```toml
version = 1

[defaults]
password_file = "/etc/ghostsnap/password"

[jobs.website-b2]
repository = "s3:my-b2-bucket/web-01"
paths = ["/etc/nginx", "/var/www", "/etc/letsencrypt"]
extra_paths = ["/var/backups/ghostsnap/web-01"]
tags = ["host:web-01", "service:nginx", "env:prod"]
exclude = ["*/cache/*", "*/tmp/*", "*.log", "node_modules", ".git"]

pre_hook = """
set -e
mkdir -p /var/backups/ghostsnap/web-01
mysqldump --single-transaction appdb > /var/backups/ghostsnap/web-01/appdb.sql
"""
post_hook = "rm -rf /var/backups/ghostsnap/web-01"
pre_hook_timeout = "10m"
shell = "/bin/bash"

keep_daily = 7
keep_weekly = 4
keep_monthly = 6
prune = true
```

Run it with:

```bash
ghostsnap job --config /etc/ghostsnap/jobs.toml run website-b2
```

Additional ready-to-use configurations are in the
[examples](../examples/) directory:

- [Website to B2](../examples/website-b2.toml)
- [Docker Compose](../examples/docker-compose.toml)

## See Also

- [Backup](backup.md) - Creating backups
- [Snapshots](snapshots.md) - Managing snapshots
- [Automation](../guides/automation.md) - Cron and systemd scheduling
