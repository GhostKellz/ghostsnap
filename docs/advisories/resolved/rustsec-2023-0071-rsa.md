# RUSTSEC-2023-0071 — `rsa` Marvin Attack

- **Advisory:** https://rustsec.org/advisories/RUSTSEC-2023-0071
- **Crate:** `rsa` (all versions, including `0.10.0-rc.x`)
- **Severity:** Medium (5.9)
- **Status:** Resolved — dependency removed

## Summary

The `rsa` crate is vulnerable to the "Marvin Attack," a timing side-channel in
RSA private-key operations (decryption/signing) that can allow key recovery by an
attacker able to measure operation timing. There is **no patched release** of the
`rsa` crate.

## How it reached Ghostsnap

The native SFTP backend uses `russh`, which enables RSA support by default:

```
rsa 0.10.0-rc.18
├── russh        (feature "rsa")
└── ssh-key      (feature "rsa", via russh)
```

`russh`'s default features are `["flate2", "aws-lc-rs", "rsa"]`. The `rsa` feature
gates `dep:rsa`, `dep:pkcs1`, and `ssh-key/rsa`.

## Resolution

RSA support is disabled by opting out of `russh`'s default features and enabling
only what we need:

```toml
# Cargo.toml (workspace)
russh = { version = "0.61", default-features = false, features = ["flate2", "aws-lc-rs"] }
```

This removes the `rsa` crate from the dependency tree entirely.

### Behavioral impact

SFTP now uses **Ed25519/ECDSA** for both host-key verification and client-key
authentication. RSA host keys and RSA client keys are not supported. This matches
the project's preference for modern key types; virtually all servers from the last
decade present Ed25519 host keys by default.

Client key search order (`core/src/storage.rs`):

1. `$SFTP_KEY_FILE` if set
2. `~/.ssh/id_ed25519`
3. `~/.ssh/id_ecdsa`

## Verification

```bash
$ cargo tree -i rsa --target all
error: package ID specification `rsa` did not match any packages
```
