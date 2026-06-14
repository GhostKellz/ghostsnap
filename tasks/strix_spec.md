# Strix Integration Spec

## Goal

Define how Ghostsnap should integrate with Strix as a practical S3-compatible backend target, without pretending Strix is a special native backend.

The right near-term model is:

- Ghostsnap treats Strix as an S3-compatible repository target
- Ghostsnap does not grow a dedicated `strix:` backend scheme yet
- Strix-specific guidance lives in docs and validation notes, not in a custom protocol layer

## Why Strix Matters To Ghostsnap

Ghostsnap now supports native `s3:bucket/prefix` repositories.

Strix is relevant because it is an S3-compatible object storage server written in Rust that can serve as:

- a self-hosted target for Ghostsnap repositories
- a local/dev validation environment for S3 behavior
- a possible future “recommended self-hosted backend” if compatibility becomes strong enough

## Integration Model

### Phase 1: S3-Compatible Target Only

Ghostsnap should integrate with Strix through the existing S3 repository path:

- `ghostsnap init --backend s3 --bucket <bucket> --endpoint <strix-endpoint> --repo s3:<bucket>/<prefix>`
- `ghostsnap backup --repo s3:<bucket>/<prefix> ...`
- `ghostsnap snapshots --repo s3:<bucket>/<prefix>`
- `ghostsnap restore --repo s3:<bucket>/<prefix> ...`
- `ghostsnap check --repo s3:<bucket>/<prefix> --read-data`

No Strix-specific backend type is needed for this phase.

### Phase 2: Compatibility Guidance

If Strix validation goes well, Ghostsnap should add documented guidance for:

- endpoint configuration
- region behavior if Strix ignores or simplifies AWS region semantics
- credential setup
- SSE compatibility expectations
- known behavior differences versus AWS S3

### Phase 3: Optional Ghostsnap Convenience Layer

Only if repeated real-world use justifies it, Ghostsnap could later add one of:

- a `--provider strix` convenience flag that still maps to S3 behavior
- a short provider guide in docs

It should not add a dedicated backend implementation unless Strix requires behavior that meaningfully diverges from generic S3.

## Current Assumptions About Strix

Based on the current Strix repo state:

- it is a single-node Rust S3-compatible object storage server
- it is promising but still PoC / not fully production-hardened
- it looks strongest in local single-node S3/IAM/admin fundamentals
- some advanced claims appear ahead of fully proven operational maturity

That means Ghostsnap should treat Strix as:

- a valid S3-compatible validation target
- not yet a special-cased first-class integration with bespoke code paths

## Ghostsnap Requirements For Strix Compatibility

For Ghostsnap to consider Strix a supported compatibility target, these flows should pass end-to-end:

- repository init to Strix-backed S3 bucket
- backup of real directory trees
- snapshot listing
- stats
- `check --read-data`
- restore of real files
- copy local -> Strix-backed S3
- eventually copy Strix-backed S3 -> local and Strix-backed S3 -> Strix-backed S3

## Compatibility Questions To Validate

- Does Strix accept Ghostsnap's current object naming and write patterns cleanly?
- Does Strix behave correctly for the object listing patterns Ghostsnap uses for:
  - `keys/`
  - `snapshots/`
  - `data/`
  - `index/`
- Does Strix preserve consistency strongly enough for immediate reopen/list/check flows?
- Do endpoint and region handling semantics match Ghostsnap's current S3 client assumptions?
- Are SSE-related expectations compatible, especially if Ghostsnap persists SSE intent in repo config?
- Are there multipart or large-object edge cases that matter for bigger pack files?

## What Ghostsnap Should Not Do Yet

- do not add a dedicated `strix:` repository scheme
- do not fork the S3 storage layer into a Strix-specific backend
- do not market Strix compatibility as guaranteed until Ghostsnap runs a real validation matrix against it
- do not claim Strix as a production recommendation until both projects have stronger operational evidence

## Practical Validation Plan

### Minimum Validation

- run Ghostsnap's opt-in S3 integration test against Strix
- perform one manual workflow:
  - init
  - backup
  - snapshots
  - stats
  - `check --read-data`
  - restore

### Stronger Validation

- verify local -> Strix S3 copy
- verify Strix S3 -> local copy
- verify Strix S3 -> Strix S3 copy
- verify larger backup packs
- verify behavior under restart/reopen conditions

## Definition Of Done For Initial Strix Support

Ghostsnap can honestly say “tested with Strix” when:

- Strix works as an S3-compatible endpoint with Ghostsnap's native `s3:` repositories
- the documented validation matrix has been executed successfully
- any Strix-specific caveats are documented plainly

Before that point, Strix should be described only as a promising S3-compatible target to test.
