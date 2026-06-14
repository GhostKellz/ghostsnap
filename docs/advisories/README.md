# Security Advisories

This directory records `cargo audit` advisories that have affected Ghostsnap and
how they were resolved. The goal is a clean `cargo audit` with **no ignored
advisories** — issues are fixed by removing the offending dependency rather than
suppressed.

## Resolved

| Advisory | Crate | Resolution |
|----------|-------|------------|
| [RUSTSEC-2023-0071](resolved/rustsec-2023-0071-rsa.md) | `rsa` | Disabled the `rsa` feature in `russh`; SFTP is Ed25519/ECDSA-only. |
| [RUSTSEC-2023-0089](resolved/rustsec-2023-0089-atomic-polyfill.md) | `atomic-polyfill` | Dropped postcard's default `heapless-cas` feature; `heapless`/`atomic-polyfill` no longer pulled in. |

## Verifying

```bash
cargo audit
```

A clean run scans the dependency tree and reports no vulnerabilities or warnings.
To confirm a specific crate is absent from the tree (including platform-specific
dependencies):

```bash
cargo tree -i <crate> --target all
```
