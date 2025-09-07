# Ghostsnap Repository Format Specification

**Version:** 1  
**Status:** Draft  
**Date:** 2025-09-07

## Overview

Ghostsnap uses a content-addressed storage format inspired by restic but with modern cryptographic primitives and optimizations for cloud backends. This document defines the on-disk and network storage format.

## Repository Layout

A Ghostsnap repository consists of the following directory structure:

```
repository/
├── config              # Repository configuration (JSON)
├── keys/               # Encrypted master keys
│   └── <key-id>        # Key file (JSON)
├── data/               # Pack files containing encrypted chunks
│   └── <pack-id>       # Pack file (binary)
├── index/              # Index files mapping chunks to packs
│   └── <index-id>      # Index file (JSON)
├── snapshots/          # Snapshot metadata
│   └── <snapshot-id>   # Snapshot file (JSON, encrypted)
└── locks/              # Repository locks
    └── <lock-id>       # Lock file (JSON)
```

## File Formats

### Repository Configuration (`config`)

JSON format containing repository metadata:

```json
{
  "version": 1,
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "chunker_polynomial": "0x3DA3358B4DC173",
  "kdf_params": {
    "algorithm": "argon2id",
    "iterations": 1,
    "memory": 65536,
    "parallelism": 4,
    "salt": "base64-encoded-salt"
  }
}
```

### Key Files (`keys/<key-id>`)

JSON format containing encrypted data encryption keys:

```json
{
  "encrypted_key": "base64-encoded-encrypted-key",
  "kdf_params": {
    "algorithm": "argon2id",
    "iterations": 1,
    "memory": 65536,
    "parallelism": 4,
    "salt": "base64-encoded-salt"
  }
}
```

### Pack Files (`data/<pack-id>`)

Binary format containing encrypted chunks:

```
[Chunk Data 1][Chunk Data 2]...[Chunk Data N][Pack Index][Index Length][Checksum]
```

- **Chunk Data**: Encrypted chunks concatenated together
- **Pack Index**: JSON metadata about chunks in this pack
- **Index Length**: 4 bytes (little-endian) indicating pack index size
- **Checksum**: 32 bytes BLAKE3 hash of everything before the checksum

Pack Index JSON format:
```json
{
  "chunks": [
    {
      "id": "blake3-hash-hex",
      "offset": 0,
      "length": 1024,
      "uncompressed_length": 1024
    }
  ]
}
```

### Index Files (`index/<index-id>`)

JSON format mapping chunk IDs to their locations:

```json
{
  "chunks": {
    "blake3-hash-hex": {
      "id": "blake3-hash-hex",
      "pack_id": "pack-uuid",
      "offset": 1024,
      "length": 2048,
      "uncompressed_length": 2048
    }
  },
  "packs": {
    "pack-uuid": {
      "id": "pack-uuid",
      "size": 67108864,
      "chunk_count": 123
    }
  }
}
```

### Snapshot Files (`snapshots/<snapshot-id>`)

Encrypted JSON format containing snapshot metadata:

```json
{
  "id": "snapshot-uuid",
  "parent": "parent-snapshot-uuid",
  "tree": "blake3-hash-of-tree-object",
  "paths": ["/home/user", "/etc"],
  "hostname": "myserver",
  "username": "user",
  "time": "2025-09-07T12:00:00Z",
  "tags": ["daily", "important"],
  "excludes": ["*.tmp", ".git"]
}
```

### Tree Objects

Tree objects represent directory structures and are stored as encrypted chunks. JSON format:

```json
{
  "nodes": [
    {
      "name": "file.txt",
      "node_type": "File",
      "mode": 33188,
      "uid": 1000,
      "gid": 1000,
      "size": 1024,
      "mtime": 1694091600,
      "subtree_id": null,
      "chunks": ["blake3-hash-1", "blake3-hash-2"]
    },
    {
      "name": "subdir",
      "node_type": "Directory",
      "mode": 16877,
      "uid": 1000,
      "gid": 1000,
      "size": 0,
      "mtime": 1694091600,
      "subtree_id": "blake3-hash-of-subtree",
      "chunks": []
    }
  ]
}
```

## Cryptographic Design

### Key Derivation

- **Master Key**: Derived from user password using Argon2id
- **Data Encryption Key**: Random 256-bit key, encrypted with master key
- **Key Rotation**: New data keys can be generated, old keys remain valid

### Encryption

- **Algorithm**: ChaCha20-Poly1305 (AEAD)
- **Key Size**: 256 bits
- **Nonce**: 96 bits (random, prepended to ciphertext)
- **Authentication**: Built into ChaCha20-Poly1305

### Hashing

- **Content Addressing**: BLAKE3 (256-bit output)
- **Pack Checksums**: BLAKE3 (256-bit output)

## Content Chunking

### FastCDC Parameters

- **Minimum Size**: 1/4 of average (default: 1 MiB)
- **Average Size**: Configurable (default: 4 MiB)  
- **Maximum Size**: 4x average (default: 16 MiB)
- **Polynomial**: Stored in repository config for consistency

### Deduplication

- Content-addressed storage using BLAKE3 hashes
- Global deduplication across all snapshots
- Chunk boundaries determined by content, not file boundaries

## Concurrency and Locking

### Repository Locks

Lock files prevent concurrent modifications:

```json
{
  "hostname": "myserver",
  "pid": 12345,
  "created": "2025-09-07T12:00:00Z",
  "expires": "2025-09-07T12:30:00Z"
}
```

### Atomic Operations

- Pack files are written atomically (temp file + rename)
- Index files are append-only until compaction
- Snapshots are written after all referenced data is committed

## Backend Compatibility

### Local Filesystem

- Direct file operations
- Atomic rename for consistency
- File locking for concurrency control

### Cloud Storage (S3/Azure/B2)

- Object-based storage model
- Conditional writes for atomic operations  
- Prefix-based organization for efficient listing
- Server-side encryption options (SSE-S3, SSE-KMS, etc.)

## Migration and Versioning

### Version Detection

Repository version is stored in the `config` file. Unsupported versions cause immediate failure.

### Forward Compatibility

- Unknown fields in JSON are ignored
- New file types can be added without breaking existing clients
- Chunking parameters are stored per-repository for consistency

### Backward Compatibility

Major version changes may break compatibility. Migration tools will be provided for significant format changes.

## Security Considerations

### Threat Model

- **Data Confidentiality**: All content is encrypted with ChaCha20-Poly1305
- **Data Integrity**: BLAKE3 checksums protect against corruption
- **Authentication**: Poly1305 MAC prevents tampering
- **Key Security**: Master keys never stored unencrypted

### Attack Resistance

- **Chosen Plaintext**: Random nonces prevent deterministic encryption
- **Known Plaintext**: Content-defined chunking reduces predictability  
- **Metadata Leakage**: File sizes and access patterns may be observable
- **Side Channel**: Implementation uses constant-time crypto primitives

## Performance Characteristics

### Space Efficiency

- Global deduplication across snapshots
- Compressed pack files (optional, future)
- Delta compression for similar content (future)

### Time Complexity

- **Backup**: O(n) in changed data size
- **Restore**: O(n) in restored data size  
- **List**: O(1) for snapshot metadata
- **Dedup Check**: O(1) with proper indexing

### Network Efficiency

- Incremental uploads (only new chunks)
- Parallel transfers for large operations
- Range requests for partial pack reads
- Bandwidth limiting and retry logic

## Implementation Notes

### Chunk Storage

Chunks are stored in pack files to reduce object count and improve performance on object storage backends.

### Index Management

- Multiple index files are merged during reads
- Periodic compaction reduces index fragmentation  
- Bloom filters may be added for faster existence checks

### Error Handling

- Corrupted packs are detected via BLAKE3 checksums
- Missing chunks cause partial restore with clear error messages
- Repository consistency can be verified and repaired

---

This specification is a living document and will evolve as the Ghostsnap implementation matures.