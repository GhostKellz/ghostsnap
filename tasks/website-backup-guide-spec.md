# Website Backup Guide Spec

## Purpose

This is an internal implementation/content spec for a future operator guide under `docs/`.

The guide should help operators back up and restore a typical self-hosted website stack with Ghostsnap:

- Ubuntu or Debian-style host
- Nginx configuration
- site content under `/var/www`
- app environment/config files
- TLS material when applicable
- database dumps staged before backup
- local, B2-via-S3, Azure, or rclone targets

This is not a feature spec for a dedicated `ghostsnap website` command.

## Audience

- small hosting operators
- homelab/self-hosted users
- VPS admins
- anyone backing up traditional web servers without Kubernetes-level complexity

## Guide Goals

- show what to back up
- show what not to trust to file-level backup alone
- show a reliable nightly backup pattern
- show restore steps for common incidents
- keep the workflow realistic and simple

## Scope

### In Scope

- `/etc/nginx/`
- `/etc/nginx/conf.d/`
- `/etc/nginx/sites-available/`
- `/etc/nginx/sites-enabled/`
- `/var/www/`
- app `.env` files where appropriate
- TLS/certificate material such as `/etc/letsencrypt/`
- staging database dumps before backup
- Ghostsnap backup, snapshot listing, check, restore, forget, prune

### Out Of Scope

- full HA/load-balanced infrastructure
- Kubernetes
- database replication design
- app-specific deployment systems

## Core Message

For websites, Ghostsnap should back up:

- web content
- web server configuration
- supporting secrets/config files
- generated database dumps

Ghostsnap should not pretend a live database directory backup is a safe substitute for a consistent logical dump.

## Required Sections For Future Docs Guide

### 1. What To Back Up

Must cover:

- site content under `/var/www`
- Nginx configuration paths
- TLS files if managed locally
- deployment/config files such as Compose files or systemd units if relevant
- application secrets and env files, with caution about permissions

### 2. Database Dump First

Must explain:

- MySQL/MariaDB: use `mysqldump` or equivalent
- PostgreSQL: use `pg_dump` or `pg_dumpall`
- stage dump files into a temporary directory
- back up that directory with Ghostsnap
- clean it up after success

### 3. Recommended Backup Layout

Recommend a staged approach like:

- `/var/backups/ghostsnap/<job-name>/` for dumps/manifests
- Ghostsnap backs up:
  - `/etc/nginx`
  - `/var/www`
  - `/etc/letsencrypt`
  - staging dump directory

### 4. Example Targets

Must include examples for:

- local repository
- Backblaze B2 via S3 compatibility
- Azure Blob

Optional:

- rclone remote example

### 5. Scheduling

Must include examples for:

- cron
- systemd timer/service

### 6. Restore Walkthroughs

Must include:

- restore full site content to a staging directory
- restore just Nginx config
- restore just one database dump
- verify restored files before cutting over

## Example Workflows To Capture

### Workflow A: Standard Ubuntu Nginx Host

Paths:

- `/etc/nginx`
- `/var/www`
- `/etc/letsencrypt`
- `/var/backups/ghostsnap/site-01`

Pre-backup step:

- create DB dump into staging directory

Backup step:

- `ghostsnap --repo <repo> backup ...`

Post-backup step:

- cleanup staging directory

### Workflow B: PHP/Laravel Or WordPress Style Site

Paths:

- app content
- uploads/media
- env/config files
- DB dump staging directory

Special note:

- call out writable cache/session paths that may not need long-term retention

### Workflow C: Reverse Proxy + Static Sites

Paths:

- `/etc/nginx`
- static site roots
- certificates

Special note:

- simplest restore case; good quickstart example

## Restore Expectations To Document

### Full Host Rebuild

1. install OS and Ghostsnap
2. initialize access to repository
3. restore to staging path
4. verify Nginx configs
5. restore `/var/www`
6. restore certificates carefully with permissions preserved
7. import DB dump
8. restart services

### Partial Restore

1. list snapshot contents
2. restore specific path
3. validate content before replacing live files

### Rollback

1. identify prior snapshot
2. restore only affected paths
3. compare before replacing live tree

## Guidance To Include

- use tags like `host:web-01`, `service:nginx`, `type:website`, `env:prod`
- prefer restoring to a temporary location before replacing live files
- test `ghostsnap check` regularly
- run `forget` and `prune` on a schedule
- document credential handling for B2/Azure in automation

## Future Tie-In With Job Configs

Once job/config support exists, this guide should include:

- one example TOML job for B2
- one example TOML job for Azure
- a pre-hook for database dump generation
- a post-hook for cleanup

## Acceptance Criteria For The Future Docs Guide

- guide is operational, not marketing
- examples are copyable and realistic
- backup and restore are both covered
- B2 and Azure examples are included
- no special product surface is invented for websites
