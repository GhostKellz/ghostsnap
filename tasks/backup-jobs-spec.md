# Ghostsnap Backup Jobs Spec

## Goal

Add a job-driven workflow layer on top of Ghostsnap so operators can run reliable, repeatable backups without writing long shell commands every night.

This feature should make Ghostsnap feel closer to an easier restic for real server operations:

- websites on standard Ubuntu hosts
- Nginx configs and application content
- Docker bind mounts and named volumes
- pre-backup database dumps
- offsite backup targets like Backblaze B2, Azure Blob, S3-compatible storage, and rclone-backed providers

This should stay lightweight and operator-friendly. Avoid turning Ghostsnap into a full orchestration platform.

## Product Direction

- Keep Ghostsnap as a backup CLI first.
- Add a small job/config layer for repeatable operations.
- Prefer simple declarative jobs plus optional hooks.
- Do not add backend-specific job semantics where a normal repository URI already works.

## Proposed User Model

Operators define one or more backup jobs in a local TOML file.

Examples:

- `nightly-web`
- `docker-wordpress`
- `hestia-host`
- `hudu-selfhosted`

Then run:

```bash
ghostsnap job run nightly-web
ghostsnap job run docker-wordpress
ghostsnap job run --all
ghostsnap job validate nightly-web
ghostsnap job list
```

## Proposed CLI Surface

### New Top-Level Command

```bash
ghostsnap job list
ghostsnap job validate <job-name>
ghostsnap job run <job-name>
ghostsnap job run --all
ghostsnap job show <job-name>
```

### Global Job File Selection

```bash
ghostsnap --config /etc/ghostsnap/jobs.toml job run nightly-web
```

Default search order can be decided later, but a reasonable first pass is:

1. `--config <path>`
2. `GHOSTSNAP_CONFIG`
3. `/etc/ghostsnap/jobs.toml`
4. `./ghostsnap.toml`

If that feels too magic, require `--config` in v1 of the feature.

## TOML Format

### Top-Level Shape

```toml
version = 1

[defaults]
repository = "s3:my-bucket/backups"
password_env = "GHOSTSNAP_PASSWORD"

[jobs.nightly-web]
repository = "s3:website-backups/prod"
paths = ["/etc/nginx", "/var/www", "/etc/letsencrypt"]
tags = ["host:web-01", "service:nginx", "type:website", "env:prod"]
exclude = ["*.tmp", "*/cache/*"]
pre_hook = """
mkdir -p /var/backups/ghostsnap/web-01
mysqldump --defaults-extra-file=/root/.my.cnf --single-transaction appdb > /var/backups/ghostsnap/web-01/appdb.sql
"""
post_hook = "rm -rf /var/backups/ghostsnap/web-01"
extra_paths = ["/var/backups/ghostsnap/web-01"]
keep_daily = 7
keep_weekly = 4
keep_monthly = 6
prune = true
```

### Suggested Fields

#### Core Job Fields

- `repository = "azure:account/container/prefix"`
- `password_env = "GHOSTSNAP_PASSWORD"`
- `password_file = "/etc/ghostsnap/password"`
- `paths = ["/var/www", "/etc/nginx"]`
- `extra_paths = ["/var/backups/ghostsnap"]`
- `tags = ["host:web-01", "service:nginx"]`
- `exclude = ["*.tmp", "node_modules"]`
- `exclude_if_present = [".nobackup"]`
- `one_file_system = true`
- `max_file_size = "10G"`
- `hostname = "web-01"`

#### Hook Fields

- `pre_hook = "..."`
- `post_hook = "..."`
- `pre_hook_timeout = "10m"`
- `post_hook_timeout = "5m"`
- `shell = "/bin/bash"`
- `working_directory = "/root"`

#### Retention Fields

- `keep_last = 10`
- `keep_hourly = 24`
- `keep_daily = 7`
- `keep_weekly = 4`
- `keep_monthly = 6`
- `keep_yearly = 2`
- `prune = true`

#### Safety Fields

- `require_paths_exist = true`
- `stop_on_pre_hook_failure = true`
- `skip_if_repo_unreachable = false`
- `dry_run = false`

## Hook Model

Hooks are the key feature for real server backups.

### Pre-Hook Use Cases

- dump MySQL or PostgreSQL databases
- export app state to a staging directory
- stop or pause non-critical containers
- generate a consistent manifest of backed-up services

### Post-Hook Use Cases

- clean temporary dump directories
- restart paused services
- send completion notifications later if desired

### Hook Rules

- hooks execute on the local host
- hooks are optional
- non-zero exit from `pre_hook` should fail the job by default
- `post_hook` should still have a configurable chance to run after failure for cleanup
- stdout/stderr should be shown clearly in job output

## Execution Model

### `job validate`

This command should not back up data. It should validate:

- config file parses cleanly
- required fields exist
- repository URI parses
- password source is resolvable
- source paths exist
- hook shell is available
- hook commands are syntactically runnable

Optional future validation:

- remote repository connectivity
- cloud credential presence
- write permissions to staging directories

### `job run`

Execution order should be:

1. load config
2. validate selected job
3. run `pre_hook` if configured
4. run `ghostsnap backup` with job fields
5. run retention (`forget`) if configured
6. run `prune` if enabled
7. run `post_hook`
8. print a final summary

### Suggested Summary Output

At the end of a job, print:

- job name
- repository
- snapshot ID
- tags
- duration
- bytes processed
- retention actions taken
- prune result
- hook success/failure

## Operator Examples

### Example: Ubuntu Website Host To Backblaze B2

```toml
version = 1

[jobs.website-b2]
repository = "s3:my-b2-bucket/web-01"
password_file = "/etc/ghostsnap/password"
paths = ["/etc/nginx", "/var/www", "/etc/letsencrypt"]
extra_paths = ["/var/backups/ghostsnap/web-01"]
tags = ["host:web-01", "service:web", "type:website", "target:b2"]
exclude = ["*/cache/*", "*.tmp"]
pre_hook = """
mkdir -p /var/backups/ghostsnap/web-01
mysqldump --defaults-extra-file=/root/.my.cnf --single-transaction appdb > /var/backups/ghostsnap/web-01/appdb.sql
"""
post_hook = "rm -rf /var/backups/ghostsnap/web-01"
keep_daily = 7
keep_weekly = 4
keep_monthly = 6
prune = true
```

Expected environment for B2:

```bash
export AWS_ACCESS_KEY_ID="..."
export AWS_SECRET_ACCESS_KEY="..."
export AWS_ENDPOINT_URL="https://s3.us-west-004.backblazeb2.com"
```

### Example: Ubuntu Website Host To Azure Blob

```toml
version = 1

[jobs.website-azure]
repository = "azure:mystorageaccount/website-backups/web-01"
password_file = "/etc/ghostsnap/password"
paths = ["/etc/nginx", "/var/www", "/etc/letsencrypt"]
extra_paths = ["/var/backups/ghostsnap/web-01"]
tags = ["host:web-01", "service:web", "type:website", "target:azure"]
pre_hook = """
mkdir -p /var/backups/ghostsnap/web-01
pg_dump -Fc myapp > /var/backups/ghostsnap/web-01/myapp.dump
"""
post_hook = "rm -rf /var/backups/ghostsnap/web-01"
keep_daily = 7
keep_weekly = 4
prune = true
```

Expected environment:

```bash
export AZURE_STORAGE_ACCOUNT="mystorageaccount"
export AZURE_STORAGE_KEY="..."
```

### Example: Docker Compose Application

```toml
version = 1

[jobs.docker-compose-app]
repository = "s3:infra-backups/docker-app-01"
password_file = "/etc/ghostsnap/password"
paths = ["/opt/myapp/docker-compose.yml", "/opt/myapp/.env", "/srv/myapp/data"]
extra_paths = ["/var/backups/ghostsnap/myapp"]
tags = ["host:app-01", "service:docker", "stack:myapp"]
pre_hook = """
mkdir -p /var/backups/ghostsnap/myapp
docker exec myapp-db pg_dump -U postgres myapp > /var/backups/ghostsnap/myapp/postgres.sql
"""
post_hook = "rm -rf /var/backups/ghostsnap/myapp"
keep_daily = 7
keep_weekly = 4
prune = true
```

## Hudu Future Direction

Hudu should be treated as an operator workflow guide, not a dedicated Ghostsnap feature.

Current known Hudu workflow:

- use Hudu's built-in Docker/Postgres backup process
- generate a database dump with their documented command
- back up local object/file storage separately
- keep matching environment/config material with the backup set

From Hudu's current self-hosted docs:

- manual Postgres dump example:
  - `sudo docker compose exec -T db pg_dump -U postgres hudu_production > NAME-OF-DUMP.sql`
- local file storage path example:
  - `/var/lib/docker/volumes/hudu2_app_data/_data/`

Planned Ghostsnap position for Hudu:

- no `ghostsnap hudu` command
- add a future operator guide under `docs/`
- recommend using Hudu's native dump/export process in a pre-hook
- then use Ghostsnap to back up:
  - generated SQL dump
  - Hudu file storage
  - compose/env/config material

Possible future Hudu job example:

```toml
[jobs.hudu-selfhosted]
repository = "s3:ops-backups/hudu"
password_file = "/etc/ghostsnap/password"
paths = ["/opt/hudu/docker-compose.yml", "/opt/hudu/.env", "/var/lib/docker/volumes/hudu2_app_data/_data"]
extra_paths = ["/var/backups/ghostsnap/hudu"]
tags = ["service:hudu", "type:documentation", "host:ops-01"]
pre_hook = """
mkdir -p /var/backups/ghostsnap/hudu
cd /opt/hudu && sudo docker compose exec -T db pg_dump -U postgres hudu_production > /var/backups/ghostsnap/hudu/hudu_production.sql
"""
post_hook = "rm -rf /var/backups/ghostsnap/hudu"
keep_daily = 7
keep_weekly = 4
prune = true
```

## Non-Goals For V1

- no service-specific subcommands like `ghostsnap hudu backup`
- no Docker daemon orchestration engine
- no embedded cron scheduler
- no notification framework yet
- no backend-specific job types

## Recommended Implementation Order

1. add config file parsing and `job list`
2. add `job validate`
3. add `job run` for backup-only jobs
4. add pre/post hooks
5. add retention and prune integration
6. add docs and real operator examples
7. add binary/integration tests for job workflows

## Validation Requirements

Before calling this feature done, verify:

- local job file parsing
- backup job execution against a local repository
- backup job execution against at least one remote target
- hook success and hook failure behavior
- retention/prune wiring
- docs examples match real CLI behavior
