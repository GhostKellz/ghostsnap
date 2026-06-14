# RUSTSEC-2023-0089 — `atomic-polyfill` Unmaintained

- **Advisory:** https://rustsec.org/advisories/RUSTSEC-2023-0089
- **Crate:** `atomic-polyfill` 1.0.3
- **Severity:** Warning (unmaintained)
- **Status:** Resolved — dependency removed

## Summary

`atomic-polyfill` is unmaintained. It is a build-time polyfill that provides
atomic operations on targets lacking native atomics (e.g. some embedded
`thumbv6` cores). It is never compiled for `x86_64-unknown-linux-gnu`, but
`cargo audit` scans `Cargo.lock` regardless of target, so it was still flagged.

## How it reached Ghostsnap

`postcard` (used by `ghostsnap-core` for serialization) enables the `heapless-cas`
feature by default, which pulls in `heapless` 0.7, which conditionally depends on
`atomic-polyfill`:

```
atomic-polyfill 1.0.3   (only on targets without native atomics)
└── heapless 0.7.17     (postcard default feature "heapless-cas")
    └── postcard 1.1.3
        └── ghostsnap-core
```

postcard's features:

```toml
default = ["heapless-cas"]
heapless-cas = ["heapless", "dep:heapless", "heapless/cas"]
alloc = ["serde/alloc", ...]   # does NOT require heapless
```

Ghostsnap only uses `postcard::to_allocvec` and `postcard::from_bytes`, which need
the `alloc` feature — not `heapless`.

## Resolution

Disable postcard's default features and enable only `alloc`:

```toml
# core/Cargo.toml
postcard = { version = "1.1", default-features = false, features = ["alloc"] }
```

This removes `heapless` (and therefore `atomic-polyfill`) from the dependency tree
entirely, with no change to Ghostsnap's serialization behavior.

## Verification

```bash
$ cargo tree -i heapless --target all
error: package ID specification `heapless` did not match any packages

$ cargo tree -i atomic-polyfill --target all
error: package ID specification `atomic-polyfill` did not match any packages
```
