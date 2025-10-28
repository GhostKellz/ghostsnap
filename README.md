# Ghostsnap

<div align="center">
  <img src="assets/ghostsnap-logo.png" alt="Ghostsnap Logo" width="128" height="128">

**Backup CLI Tool for Modern Infrastructure**

![rust](https://img.shields.io/badge/Rust-1.80+-brown?logo=rust)
![status](https://img.shields.io/badge/status-alpha-orange)
![license](https://img.shields.io/badge/license-MIT-blue)
![ci](https://img.shields.io/github/actions/workflow/status/ghostkellz/ghostsnap/ci.yml)
![issues](https://img.shields.io/github/issues/ghostkellz/ghostsnap)
![prs](https://img.shields.io/github/issues-pr/ghostkellz/ghostsnap)

</div>

---

## Overview

**Ghostsnap** is a fast, reliable, and developer-friendly backup CLI tool designed for modern cloud and self-hosted infrastructure. Think *restic, but simpler to configure and tailored for MinIO, Azure, Backblaze/Wasabi, and HestiaCP*.

Built in **Rust** for performance and safety, Ghostsnap makes it easy to:

* Back up websites, application data, and configs.
* Push to S3-compatible stores (MinIO, Wasabi, Backblaze, etc.).
* Integrate directly with **HestiaCP** for daily site backups.
* Hook into **Zeke** and other Ghost projects for orchestration.

---

## Features

* 🔒 **Secure by default** – client-side encryption with modern ciphers.
* 🪶 **Lightweight CLI** – single binary, no external runtime deps.
* ☁️ **Cloud-native** – supports S3, Azure Blob, Backblaze, Wasabi.
* 🗂 **Incremental snapshots** – deduplication & versioning.
* ⏱ **Scheduled backups** – cron/systemd-ready.
* 🧩 **Pluggable architecture** – extend storage backends easily.
* 📦 **HestiaCP integration** – backup sites, DBs, configs directly.

---

## Installation

```bash
# Clone the repo
git clone https://github.com/ghostkellz/ghostsnap
cd ghostsnap

# Build binary
cargo build --release

# Run CLI
./target/release/ghostsnap --help
```

Prebuilt binaries (Linux, macOS, Windows) will be available on [Releases](https://github.com/ghostkellz/ghostsnap/releases).

---

## Usage

```bash
# Initialize a repo on MinIO
ghostsnap init s3:minio/ghost-backups

# Backup a HestiaCP site
ghostsnap backup /home/hestia/web/domain.com s3:minio/ghost-backups --tag=domain.com

# List snapshots
ghostsnap snapshots s3:minio/ghost-backups

# Restore from snapshot
ghostsnap restore s3:minio/ghost-backups --tag=domain.com --target=/restore/path
```

---

## Roadmap

**Current Status: Alpha (v0.1.0) → RC1 (v0.9.0)**

### Completed ✅
* [x] Core cryptography (ChaCha20-Poly1305, BLAKE3, Argon2)
* [x] Content-defined chunking (FastCDC)
* [x] Repository structure and configuration
* [x] Basic CLI scaffolding
* [x] Backend abstraction layer

### In Progress 🚧
* [ ] Complete backup/restore implementation
* [ ] Pack management system
* [ ] Index optimization
* [ ] Backend integrations (S3, Azure, MinIO)
* [ ] Comprehensive testing

### Planned for RC1 📋
* [ ] HestiaCP integration
* [ ] Full documentation
* [ ] Security audit
* [ ] Performance optimization
* [ ] Binary releases (Linux, macOS, Windows)
* [ ] Beta testing program

**📚 For detailed roadmap see**: [TODO.md](TODO.md), [RC1_SUMMARY.md](RC1_SUMMARY.md), [QUICK_START_RC1.md](QUICK_START_RC1.md), [PRIORITY_MATRIX.md](PRIORITY_MATRIX.md)

---

## Contributing

We welcome contributions! Check out [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/awesome`)
3. Commit your changes (`git commit -m 'Add awesome feature'`)
4. Push to your branch (`git push origin feature/awesome`)
5. Open a Pull Request 🚀

---

## License

Ghostsnap is licensed under the **MIT License**. See [LICENSE](LICENSE) for details.

