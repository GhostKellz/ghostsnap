# Remote Locking Spec

## Purpose

This is an internal spec for deciding and implementing safe repository locking for non-local Ghostsnap repositories.

## Current State

Local repositories use `core/src/lock.rs` with a lock file under `locks/repo.lock`.

Write-oriented CLI commands currently attempt locking only for local repositories:

- `backup`
- `forget`
- `prune`
- `copy` destination

For remote repositories, commands currently warn that locking is not supported.

That is acceptable for experimentation, but not ideal for real nightly automation or multi-host usage.

## Problem Statement

Without remote locking:

- two scheduled jobs may write to the same repository concurrently
- `prune` can race with backup/copy
- multi-host automation becomes riskier
- the product feels less dependable for offsite-first use

## Goals

- define a backend-agnostic locking model
- keep implementation simple and robust
- support stale lock detection reasonably
- avoid requiring backend-specific APIs when repository storage primitives are enough

## Recommended Direction

Implement repository-level remote locks using the existing repository storage abstraction rather than backend-native lease systems first.

That means:

- keep lock objects inside repository storage under `locks/`
- write lock metadata as JSON
- use a storage-backed create-if-absent or best-effort atomic strategy per backend

This gives one model for:

- S3
- Azure Blob
- rclone-backed remotes

## Design Constraints

### 1. Must Not Require Full Backend Rewrite

Prefer extending `RepositoryStorage` with lock-capable operations where possible.

### 2. Must Handle Stale Locks

Lock payload should include:

- hostname
- pid
- created time
- operation
- lock type
- optional instance/session UUID

### 3. Must Be Safe Enough For Real Operators

Need clear behavior for:

- second writer arrives
- lock owner crashes
- lock age exceeds timeout
- remote clock drift concerns

## Proposed Model

### Lock Location

Keep lock objects at:

- `locks/repo.lock`

Potential future expansion:

- `locks/exclusive.lock`
- `locks/shared/<id>.lock`

But start simple unless shared locks are truly needed for correctness.

### Lock Content

JSON payload fields:

- `lock_type`
- `hostname`
- `pid`
- `created_at`
- `operation`
- `instance_id`

### Acquisition Strategy

Preferred v1:

1. check if lock exists
2. if absent, write lock object
3. read back and verify ownership if needed
4. if present, inspect staleness and fail cleanly

### Release Strategy

1. delete lock object if owned
2. best-effort cleanup on normal exit
3. document that abnormal process death relies on stale-lock handling

## Backend-Specific Notes

### S3

- native atomic create is limited
- may need write-then-verify ownership pattern
- stale lock cleanup must be conservative

### Azure Blob

- Azure has leasing primitives, but that may be more complexity than needed for v1
- storage-abstraction-based lock objects are preferred first unless leases become clearly necessary

### Rclone

- weakest atomicity story
- likely limited to object/file existence plus overwrite behavior of underlying remote
- document higher risk if atomic semantics are insufficient

## Shared vs Exclusive Locks

Current local model defines both shared and exclusive, but the implementation behaves like a single exclusive lock file.

Questions to resolve:

- do we actually need shared locks in v1 remote support?
- is a single writer lock enough for the near-term product?

Recommended answer:

- v1 should prioritize safe exclusive locking for mutating operations
- read-only operations can remain unlocked initially if needed
- only add full shared-lock semantics if there is a concrete need

## CLI/UX Expectations

When a lock is held, error output should clearly report:

- operation holding the lock
- host
- pid if known
- age of lock
- whether it appears stale

Potential future command:

```bash
ghostsnap unlock --repo <repo>
```

This should not ship casually. If added, it needs strong warnings.

## Documentation Requirements Later

Future docs should explain:

- local and remote locking behavior
- single-writer guidance until remote locking is fully trusted
- how stale locks are detected
- when manual unlock is appropriate

## Validation Requirements

Need tests for:

- local lock behavior remains intact
- remote lock acquire/release on supported backends
- conflicting writer is rejected
- stale remote lock can be detected and cleaned safely
- copy/prune/forget/backup all use the new lock path consistently

## Implementation Order

1. introduce lock operations through storage abstraction
2. implement remote exclusive lock flow
3. wire mutating commands to use it
4. add tests for at least one remote backend
5. document behavior and limitations

## Acceptance Criteria

- remote mutating operations are no longer effectively unlocked
- behavior is consistent across local and remote repository concepts
- failure messages are operator-friendly
- stale lock handling is conservative and understandable
