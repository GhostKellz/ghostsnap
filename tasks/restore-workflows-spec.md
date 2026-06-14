# Restore Workflows Spec

## Purpose

This is an internal product and documentation spec for making Ghostsnap restores feel predictable, safe, and operator-friendly.

Ghostsnap already has strong restore mechanics. The next step is to turn that capability into clear workflows and, where useful, small UX improvements.

## Current State

Current restore command already supports:

- full restore
- partial path restore
- dry run
- overwrite control
- ownership/permission toggles
- xattr toggle
- sparse file restore
- post-restore hash verification
- hardlink preservation or copy fallback

Current docs cover the basics well.

## Problem Statement

Backup tools feel easy only when restore workflows are clear under stress.

Users need confident answers to:

- how do I restore one file?
- how do I restore a whole website safely?
- how do I restore a Docker volume?
- how do I inspect before replacing live data?
- how do I validate what I restored?

## Goals

- define the key operator restore workflows
- improve restore-related docs later under `docs/`
- identify any CLI polish worth adding
- keep restore safety ahead of convenience

## Primary Workflows

### 1. Single File Recovery

Use cases:

- deleted config file
- overwritten app file
- missing TLS file

Expected operator flow:

1. list snapshot contents
2. restore specific path to temporary target
3. inspect file
4. copy into live location manually

### 2. Partial Directory Recovery

Use cases:

- recover `uploads/`
- recover `documents/`
- recover one app subtree

Expected flow:

1. restore specific directory to staging path
2. compare with live tree
3. sync or swap into place

### 3. Full Service Recovery

Use cases:

- website rebuild
- full application rollback
- Docker volume rebuild

Expected flow:

1. restore snapshot to staging target
2. validate ownership, permissions, and expected layout
3. restore database dump separately if required
4. replace live data in controlled order
5. start services and verify health

### 4. Disaster Recovery To New Host

Use cases:

- host migration
- failed VPS replacement
- bare-metal rebuild

Expected flow:

1. bootstrap Ghostsnap and credentials
2. restore into staging root
3. review configs and environment differences
4. deploy into final paths
5. verify services

## UX Improvements To Evaluate

### Restore Planning Output

Potential feature:

- `ghostsnap --repo <repo> restore <snapshot> --target <path> --dry-run`

Should clearly summarize:

- directories
- files
- symlinks
- total bytes
- skipped existing files
- whether ownership/permissions/xattrs/hardlinks will be applied

### Overwrite Safety

Current behavior is reasonable, but consider future polish:

- explicit summary of skipped existing files
- clearer warning when restoring into a non-empty target without `--overwrite`

### Verify UX

Current `--verify` is useful.

Potential polish:

- clearer final verification summary
- machine-readable output later if JSON/reporting is added

### Restore Manifest

Potential future feature:

- emit a summary/report of restored paths and outcomes
- useful for audits and change review

This should not block core workflow improvements.

## Workflow-Specific Notes

### Website Restore

Need future docs covering:

- restore `/etc/nginx`
- restore `/var/www`
- restore `/etc/letsencrypt`
- restore DB dump
- validate configs before switching live

### Docker Restore

Need future docs covering:

- restore bind mount path directly
- restore named volume to temp location then copy back into volume
- restore DB dumps before app start where required

### Hudu Restore

Keep this docs-only in the future.

Expected workflow:

- restore SQL dump created by Hudu-native process
- restore file/object storage contents
- restore compose/env/config files

## Validation Requirements

Future validation should cover:

- full restore to empty directory
- partial restore to temp directory
- dry-run output sanity
- overwrite behavior
- verify behavior
- hardlink restoration
- sparse file restoration
- restore with `--no-ownership` and `--no-permissions`

## Future Docs Set To Build Later

- `docs/usage/restore.md` polish pass
- website restore guide
- Docker restore guide
- disaster recovery guide

## Acceptance Criteria

- restore guidance is practical under incident conditions
- examples favor staging restores before live replacement
- website and Docker restore paths are explicitly documented later
- any CLI polish remains small and safety-focused
