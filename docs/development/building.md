# Building Ghostsnap

## Prerequisites

### Required

- **Rust 1.90+** (2024 edition)
- **Cargo** (comes with Rust)

### Optional

- **OpenSSL dev headers** (some platforms)

## Quick Build

```bash
# Clone repository
git clone https://github.com/GhostKellz/ghostsnap.git
cd ghostsnap

# Build release binary
cargo build --release

# Binary location
./target/release/ghostsnap --version
```

## Build Configurations

### Debug Build

```bash
cargo build
# Binary: ./target/debug/ghostsnap
```

Debug builds include:

- Debug symbols
- No optimizations
- Overflow checks
- Debug assertions

### Release Build

```bash
cargo build --release
# Binary: ./target/release/ghostsnap
```

Release builds include:

- Full optimizations (LTO)
- No debug symbols
- Stripped binary

### Profile-Guided Optimization

For maximum performance:

```bash
# Build with instrumentation
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" cargo build --release

# Run typical workload
./target/release/ghostsnap --repo /tmp/repo backup ~/data

# Build with profile data
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data" cargo build --release
```

## Platform-Specific

### Linux

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build dependencies (Debian/Ubuntu)
apt install build-essential pkg-config libssl-dev

# Build dependencies (Arch)
pacman -S base-devel openssl

# Build
cargo build --release
```

### macOS

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build (no extra dependencies)
cargo build --release
```

### Windows

```powershell
# Install Rust from https://rustup.rs

# Build
cargo build --release
```

## Cross-Compilation

### Linux ARM64

```bash
# Add target
rustup target add aarch64-unknown-linux-gnu

# Install cross-compiler
apt install gcc-aarch64-linux-gnu

# Build
cargo build --release --target aarch64-unknown-linux-gnu
```

### Using Cross

```bash
# Install cross
cargo install cross

# Build for various targets
cross build --release --target aarch64-unknown-linux-gnu
cross build --release --target x86_64-unknown-linux-musl
cross build --release --target armv7-unknown-linux-gnueabihf
```

## Static Linking

For portable binaries:

```bash
# Add musl target
rustup target add x86_64-unknown-linux-musl

# Build static binary
cargo build --release --target x86_64-unknown-linux-musl

# Verify
ldd ./target/x86_64-unknown-linux-musl/release/ghostsnap
# "not a dynamic executable"
```

## Workspace Structure

```
ghostsnap/
├── Cargo.toml          # Workspace definition
├── core/               # Core library (chunking, crypto, repository)
│   ├── Cargo.toml
│   └── src/
├── backends/           # Storage backends (local, S3, Azure)
│   ├── Cargo.toml
│   └── src/
└── cli/                # CLI application
    ├── Cargo.toml
    └── src/
```

## Build Verification

```bash
# Run tests
cargo test

# Run clippy
cargo clippy -- -D warnings

# Check formatting
cargo fmt -- --check

# Build docs
cargo doc --no-deps
```
