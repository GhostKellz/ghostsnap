# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in Ghostsnap, please report it responsibly.

### How to Report

1. **Do NOT** open a public GitHub issue for security vulnerabilities
2. Email security concerns to the maintainers directly
3. Include as much detail as possible:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

### Response Timeline

- **Initial Response**: Within 48 hours
- **Assessment**: Within 7 days
- **Fix Timeline**: Depends on severity
  - Critical: 24-72 hours
  - High: 7 days
  - Medium: 30 days
  - Low: Next release cycle

## Security Model

### Cryptographic Design

Ghostsnap uses industry-standard cryptographic primitives:

| Component | Algorithm | Purpose |
|-----------|-----------|---------|
| Encryption | ChaCha20-Poly1305 (AEAD) | Data confidentiality and integrity |
| Hashing | BLAKE3 | Content addressing, deduplication |
| Key Derivation | Argon2id | Password-based key derivation |
| Random | OS CSPRNG via `rand` | Nonces, salts, key material |

### Threat Model

**What Ghostsnap protects against:**

- Unauthorized access to backup data (encryption at rest)
- Data tampering (authenticated encryption)
- Weak password attacks (Argon2id with tuned parameters)
- Deduplication attacks (encrypted chunks are content-addressed after encryption)

**What Ghostsnap does NOT protect against:**

- Compromised host system (malware with root access)
- Side-channel attacks on the backup client
- Denial of service against storage backends
- Metadata analysis (file sizes, modification times visible in encrypted snapshots)

### Key Management

- Master keys are derived from user passwords using Argon2id
- Each repository has a unique salt
- Key files are encrypted and stored in the repository
- No key escrow or recovery mechanism (password loss = data loss)

### Repository Security

- All chunk data is encrypted before storage
- Snapshots contain encrypted metadata
- Pack files include authenticated checksums
- No plaintext data leaves the client

## Security Best Practices

### For Users

1. **Use strong passwords**: Minimum 16 characters, random or passphrase
2. **Protect your password**: Use a password manager
3. **Verify repository integrity**: Run `ghostsnap check` periodically
4. **Secure storage credentials**: Use environment variables, not command-line args
5. **Keep Ghostsnap updated**: Security fixes are released as patches

### For Storage Backends

#### Local Storage
- Ensure proper filesystem permissions (700 for repository root)
- Use encrypted filesystems for additional protection

#### S3/MinIO
- Enable server-side encryption (SSE-S3 or SSE-KMS)
- Use IAM roles instead of static credentials
- Enable bucket versioning for recovery
- Configure bucket policies to restrict access

#### Azure Blob Storage
- Use managed identities where possible
- Enable soft delete for recovery
- Configure private endpoints for network isolation

## Known Limitations

1. **No Forward Secrecy**: Compromised master key exposes all backups
2. **No Key Rotation**: Changing password requires re-encrypting repository
3. **Metadata Leakage**: Encrypted snapshot headers reveal backup timestamps
4. **No MFA**: Single-factor authentication (password only)

## Security Audits

Ghostsnap has not yet undergone a formal security audit. We welcome security researchers to review the codebase.

## Dependencies

We monitor dependencies for vulnerabilities using `cargo audit`. Key cryptographic dependencies:

- `chacha20poly1305`: RustCrypto implementation
- `blake3`: Official BLAKE3 implementation
- `argon2`: RustCrypto implementation
- `rand`: Rust standard CSPRNG

`cargo audit` reports no known vulnerabilities. For the history of advisories that
were resolved by trimming transitive dependencies, see
[docs/advisories/](docs/advisories/).

## Changelog

### Security-Related Changes

See [CHANGELOG.md](CHANGELOG.md) for security-related updates.
