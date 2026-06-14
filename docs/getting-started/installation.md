# Installation

## Requirements

- Rust 1.75+ (2024 edition)
- Linux, macOS, or Windows
- For cloud backends: appropriate credentials configured

## Building from Source

```bash
# Clone the repository
git clone https://github.com/GhostKellz/ghostsnap
cd ghostsnap

# Build release binary
cargo build --release

# The binary is at target/release/ghostsnap
./target/release/ghostsnap --help
```

## Installing System-Wide

```bash
# Install to ~/.cargo/bin (in PATH)
cargo install --path cli

# Or copy manually
sudo cp target/release/ghostsnap /usr/local/bin/
```

## Shell Completions

Generate shell completions for your shell:

```bash
# Bash
ghostsnap completions bash > /etc/bash_completion.d/ghostsnap

# Zsh
ghostsnap completions zsh > ~/.zfunc/_ghostsnap

# Fish
ghostsnap completions fish > ~/.config/fish/completions/ghostsnap.fish
```

## Verifying Installation

```bash
ghostsnap --version
# ghostsnap 0.1.0

ghostsnap --help
# Shows available commands
```

## Next Steps

- [Quick Start Guide](quickstart.md) - Create your first backup
- [Configuration](configuration.md) - Set up environment variables
