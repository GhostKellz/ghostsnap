# Contributing

Contributions to Ghostsnap are welcome.

## Getting Started

1. Fork the repository
2. Clone your fork
3. Create a branch for your changes
4. Make your changes
5. Submit a pull request

## Development Setup

```bash
# Clone
git clone https://github.com/YOUR_USERNAME/ghostsnap.git
cd ghostsnap

# Build
cargo build

# Test
cargo test

# Format
cargo fmt

# Lint
cargo clippy
```

## Code Standards

### Formatting

Use `rustfmt` with default settings:

```bash
cargo fmt
```

### Linting

All code must pass clippy without warnings:

```bash
cargo clippy -- -D warnings
```

### Documentation

Public APIs should be documented:

```rust
/// Encrypts data using ChaCha20-Poly1305.
///
/// # Arguments
///
/// * `data` - The plaintext data to encrypt
/// * `key` - The 256-bit encryption key
///
/// # Returns
///
/// The encrypted data with prepended nonce and appended tag.
///
/// # Errors
///
/// Returns an error if encryption fails.
pub fn encrypt(data: &[u8], key: &Key) -> Result<Vec<u8>> {
    // ...
}
```

### Error Handling

Use `anyhow` for error propagation:

```rust
use anyhow::{Context, Result};

fn process_file(path: &Path) -> Result<()> {
    let data = fs::read(path)
        .context("Failed to read file")?;
    // ...
    Ok(())
}
```

### Testing

Add tests for new functionality:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_feature() {
        // Arrange
        let input = "test";

        // Act
        let result = new_feature(input);

        // Assert
        assert_eq!(result, expected);
    }
}
```

## Pull Request Guidelines

### Before Submitting

1. **Run tests**: `cargo test`
2. **Format code**: `cargo fmt`
3. **Run clippy**: `cargo clippy -- -D warnings`
4. **Update docs** if needed
5. **Add tests** for new features

### PR Description

Include:

- What changes were made
- Why the changes were needed
- How to test the changes
- Any breaking changes

### Commit Messages

Use conventional commits:

```
feat: add sparse file support
fix: handle symlink loops correctly
docs: update backup command documentation
test: add integration tests for restore
refactor: simplify chunking logic
```

## Architecture Decisions

For significant changes, discuss first:

1. Open an issue describing the change
2. Wait for feedback
3. Proceed with implementation

## Code Review

All PRs require review. Reviewers check for:

- Correctness
- Test coverage
- Code clarity
- Performance implications
- Security considerations

## Release Process

Releases follow semantic versioning:

- **Major**: Breaking changes
- **Minor**: New features (backwards compatible)
- **Patch**: Bug fixes

## Getting Help

- Open an issue for bugs
- Start a discussion for questions
- Check existing issues before creating new ones
