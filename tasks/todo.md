# Ghostsnap Active Todo

## Objective

Bring the current branch from "feature-complete on paper" to actually dependable for real operator use, especially around the new job/config/hook workflows.

## Current State

- `cargo test` passes.
- Local, `s3:`, `azure:`, and `rclone:` repositories are present.
- The command surface is broad and the docs are much stronger than earlier revisions.
- Config-driven jobs, hooks, and operator guides now exist.
- The main remaining risks are correctness gaps between the main backup engine and the new job layer, plus a few user-facing workflow/documentation mismatches.

## P0 Correctness Fixes

- [x] Make config-driven jobs honor all declared backup semantics.
  Current issue:
  - `ResolvedJob` carries `exclude_if_present` and `one_file_system`, but `cli/src/commands/job.rs` does not actually apply them during backup execution.
  Acceptance criteria:
  - Job execution enforces `exclude_if_present`.
  - Job execution enforces `one_file_system`.
  - Any other advertised job backup options either work or are removed from config/docs.
  **Fixed:** Added `one_file_system` support via `walker.same_file_system(true)` and `exclude_if_present` via `check_exclude_if_present()` helper.

- [x] Fix job exclude handling so it matches documented glob behavior.
  Current issue:
  - Job execution currently treats excludes like substring checks, but docs/examples use glob-style patterns such as `*/cache/*` and `*.tmp`.
  Acceptance criteria:
  - Job excludes use the same matching model as the main backup command, or docs are narrowed to the implemented behavior.
  - Add regression tests for common patterns from website and Docker examples.
  **Fixed:** Added `build_exclude_matcher()` and `should_exclude()` using `globset::GlobSet`, matching backup.rs behavior.

- [x] Fix hook timeout behavior so timed-out hooks do not leave child processes behind.
  Current issue:
  - Hooks run through a shell and timeout handling only kills the immediate child process.
  Acceptance criteria:
  - Timed-out hooks reliably terminate the launched command tree, or the limitation is explicitly documented and mitigated.
  - Add tests or documented validation for timeout cleanup behavior.
  **Fixed:** Hooks now run in their own process group via `setsid()`, and timeout kills the entire process group via `kill(-pid, SIGKILL)`.

## P0 Documentation Accuracy

- [x] Fix website restore examples to use real snapshot-relative paths.
  Current issue:
  - `docs/guides/website-backup.md` shows restore/dump examples with absolute source paths that are not how snapshot entries are stored.
  Acceptance criteria:
  - Restore examples use actual snapshot-relative paths.
  - The guide explains how paths are rooted when backing up multiple top-level sources.
  **Fixed:** Added "Understanding Snapshot Paths" section explaining relative path storage, updated all restore examples to use relative paths.

- [x] Re-audit job/config docs against actual implementation.
  Current issue:
  - README and guides now market config-driven jobs as a real feature, but parts of the job execution model still differ from the main backup command.
  Acceptance criteria:
  - README, docs, and examples reflect only the behavior that actually exists today.
  - Experimental or partial job semantics are clearly labeled if any remain.
  **Fixed:** Updated README locking claims to scope local-only, added link to locking documentation for remote guidance.

## P1 Validation

- [x] Add automated coverage for config-driven jobs.
  Current issue:
  - There are no tests covering `job list`, `job validate`, `job run`, hook execution, or retention/prune wiring through the job command.
  Acceptance criteria:
  - Add CLI-level tests for job config parsing and execution.
  - Cover at least one successful job run, one failed pre-hook, one dry-run path, and one retention/prune path.
  **Fixed:** Added 16 tests in `cli/tests/job_tests.rs` covering:
  - `job list` (2 tests)
  - `job show` (2 tests)
  - `job validate` (2 tests)
  - `job run` success, dry-run, hooks, pre-hook failure, retention (6 tests)
  - `job run --all` (1 test)
  - Glob excludes, exclude_if_present, one_file_system (3 tests)

- [x] Add regression tests for job/backup parity.
  Current issue:
  - The job runner reimplements backup traversal separately, which makes drift from the main `backup` command likely.
  Acceptance criteria:
  - Add tests proving job-driven backups preserve the intended exclude/path semantics.
  - Decide whether job execution should keep its own traversal or delegate into shared backup logic.
  **Fixed:** Added `test_job_backup_parity_with_excludes` that compares job backup output to direct backup command with same excludes. Both use globset for matching. Job traversal kept separate but now uses identical glob logic.

## P1 Reliability And Product Clarity

- [x] Resolve the remote locking story in shipped messaging.
  Current issue:
  - README still presents repository locking as a core capability while remote write commands continue to warn that locking is not supported for remote repositories.
  Acceptance criteria:
  - Either implement remote locking for mutating operations, or clearly scope locking claims to local repositories only.
  **Fixed:** README now scopes locking to local repos with link to docs. `docs/usage/repository.md` has comprehensive "Repository Locking" section with single-writer guidance.

- [x] Decide whether config-driven jobs are fully shipped or still operator-beta.
  Acceptance criteria:
  - If fully shipped, close the parity and test gaps first.
  - If still maturing, label them accordingly in README/docs until the semantics and coverage are stronger.
  **Decision:** Config-driven jobs are fully shipped. Parity with backup command verified (glob excludes, exclude_if_present, one_file_system), 16 tests covering all code paths.

## P2 Next Product Work

- [ ] After the above fixes, choose the next investment:
  Options:
  - finish operator workflow polish for websites/Docker
  - improve restore/reporting UX
  - implement remote locking
  - deepen job orchestration features

## Review Notes

- Current branch is significantly stronger than earlier reviews.
- The core repository engine is not the main concern now.
- The biggest remaining issues are concentrated in the new job/hook/operator layer and how confidently it is presented to users.

---

# Session Plan: Audit, Bugs, Backends, Docs

## Phase 1 - Dependency Audit & Azure SDK Migration
- [ ] Bump `lru` 0.12 -> 0.18 (clears RUSTSEC-2026-0002)
- [ ] Migrate Azure storage from `azure_storage_blobs` 0.21 -> `azure_storage_blob` 1.0 (+ `azure_identity` 1.0, `azure_core` 1.0) in core/src/storage.rs and backends/src/azure*.rs (clears rustls-webpki + rand advisories from azure transitive deps)
- [ ] `cargo update`; re-run `cargo audit` to confirm clean

## Phase 2 - Wire Up B2 / MinIO / SFTP Backends
- [ ] Add `RepositoryLocation::{B2, Sftp}` + URI parsing (storage.rs:57 currently rejects)
- [ ] B2 via S3-compatible endpoint mapping
- [ ] Native SFTP via russh / russh-sftp (replace stub backends/src/sftp.rs)
- [ ] init.rs handling for new backend types
- [ ] Fix unwrap panics in backend modules

## Phase 3 - Bug Fixes & Lint
- [ ] rclone stderr `read_line` -> `read_to_string` (storage.rs:897)
- [ ] main.rs init_tracing `expect` -> `try_init` (main.rs:119)
- [ ] config.rs manual_strip x3 -> `strip_suffix` (lines 360-365)
- [ ] dead_code cleanup (job_names, ResolvedJob.name)

## Phase 4 - Docs & Mermaid
- [ ] Remove stale hestia docs + references (docs/hestia/, docs/README.md:40)
- [ ] De-hestia WordPress docs
- [ ] Fix snapshots.md flag errors (--host->--hostname, --json->--format json)
- [ ] Add Mermaid diagrams (backup/restore flow, encryption hierarchy, pack format, index lookup, repo structure, job lifecycle)
- [ ] Add docs/usage/job.md
- [ ] Add b2/minio/sftp backend docs
- [ ] Update docs/README.md index

## Phase 5 - README (LAST)
- [ ] Fix install path -> target/x86_64-unknown-linux-gnu/release/ghostsnap
- [ ] Remove hestia reference
- [ ] "Built with Rust"
- [ ] Reflect new backends

## Phase 6 - Verify & Clean
- [ ] cargo build / clippy / test / audit
- [ ] Clean up /tmp scratch files
