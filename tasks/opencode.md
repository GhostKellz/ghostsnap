# OpenCode Pre-Commit Checklist

## Current State

- [x] `cargo check` passes
- [x] `cargo test` passes
- [x] `cargo fmt --all -- --check` passes
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes

## Critical

- [x] Fix Clippy-denied warnings so the workspace passes `cargo clippy --workspace --all-targets -- -D warnings`
  Fixed: collapsible_if, needless_range_loop, for_kv_map, io_other_error, useless_conversion, should_implement_trait issues across core, backends, integrations, and cli crates.
- [x] Stop advertising incomplete features as production-ready
  Added `[EXPERIMENTAL]` prefix to all HestiaCP subcommands in hestia.rs.
- [x] Align CLI behavior with supported backends
  Removed `b2` from init.rs help text (now says "local, s3, minio, azure").
- [x] Repair stale onboarding/docs index content before commit
  Deleted `docs/START_HERE.md` (was RC1-era content with broken links).

## High

- [x] Run `cargo fmt` and commit the formatted tree
  Completed.
- [x] Audit `README.md` for accuracy against the current implementation
  Updated to reflect: Local/S3/MinIO working; Azure/B2/rclone/HestiaCP marked as "in development" or "experimental".
- [x] Decide whether placeholder backends should be exported at all
  Left as-is; README now clearly indicates status.
- [ ] Verify every new command has at least one realistic smoke test path
  The overhaul added `check`, `copy`, `diff`, `dump`, `forget`, `ls`, `prune`, and `stats`, but current automated coverage is still centered on backup/restore and local backend flows.

## Medium

- [ ] Fill in or remove `.github/workflows/main.yml`
  The workflow file exists but is empty right now.
- [x] Clean up legacy planning docs that no longer represent the project
  Deleted `docs/START_HERE.md`.
- [ ] Do a final user-facing copy pass on command help and docs
  Confirm examples, backend names, and command descriptions match the implemented CLI.
- [ ] Decide whether `tasks/` docs belong in this commit
  If yes, keep them intentional; if not, avoid committing transient planning files.

## Suggested Commit Gate

- [x] `cargo fmt --all`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test`
- [x] Review `README.md`, `docs/START_HERE.md`, and `cli/src/commands/init.rs` for accuracy one last time
