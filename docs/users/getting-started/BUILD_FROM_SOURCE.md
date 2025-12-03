---
title: Building from Source
module: getting-started
id: build-from-source
order: 4
description: Build Nexus from source code
tags: [build, source, development, getting-started]
---

# Building from Source

Complete guide for building Nexus Graph Database from source code.

## Prerequisites

### Required Tools

- **Rust**: Nightly 1.85+ (edition 2024)
- **Cargo**: Rust package manager (included with Rust)
- **Git**: Version control
- **Build Tools**:
  - Linux: `build-essential`, `pkg-config`, `libssl-dev`
  - macOS: Xcode Command Line Tools
  - Windows: Visual Studio Build Tools or WSL

### Install Rust Nightly

```bash
# Install rustup if not already installed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install nightly toolchain
rustup toolchain install nightly
rustup default nightly

# Verify installation
rustc --version
cargo --version
```

## Clone Repository

```bash
git clone https://github.com/hivellm/nexus.git
cd nexus
```

## Build Process

### Development Build

```bash
# Build all packages
cargo +nightly build --workspace

# Build specific package
cargo +nightly build --package nexus-server
cargo +nightly build --package nexus-core
```

### Release Build

```bash
# Optimized release build
cargo +nightly build --release --workspace

# Build specific package in release mode
cargo +nightly build --release --package nexus-server
```

### Build Output

Binaries will be located at:
- `target/release/nexus-server` (Linux/macOS)
- `target/release/nexus-server.exe` (Windows)
- `target/release/nexus-cli` (CLI tool)

## Feature Flags

Build with specific features:

```bash
# Build with all features
cargo +nightly build --release --all-features

# Build with specific features
cargo +nightly build --release --features "mcp,umicp"
```

Available features:
- `mcp` - Model Context Protocol support
- `umicp` - Universal MCP Interface support
- `replication` - Master-replica replication
- `graph-algorithms` - Graph algorithm procedures

## Cross-Compilation

### Linux Target from macOS

```bash
# Install target
rustup target add x86_64-unknown-linux-gnu

# Build for Linux
cargo +nightly build --release --target x86_64-unknown-linux-gnu
```

### Windows Target from Linux

```bash
# Install target
rustup target add x86_64-pc-windows-gnu

# Build for Windows
cargo +nightly build --release --target x86_64-pc-windows-gnu
```

## Development Workflow

### Run Tests

```bash
# Run all tests
cargo +nightly test --workspace

# Run specific test
cargo +nightly test --package nexus-core --test integration_test

# Run with output
cargo +nightly test --workspace -- --nocapture
```

### Run Linting

```bash
# Format code
cargo +nightly fmt --all

# Run clippy
cargo +nightly clippy --workspace -- -D warnings
```

### Run Benchmarks

```bash
# Run benchmarks
cargo +nightly bench --workspace
```

## Optimization

### Release Profile

The `Cargo.toml` includes optimized release profile:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
```

### Build Time Optimization

```bash
# Use more parallel jobs
cargo +nightly build --release -j $(nproc)

# Use sccache for faster builds
cargo install sccache
export RUSTC_WRAPPER=sccache
```

## Troubleshooting

### Common Issues

**Out of Memory:**
```bash
# Increase swap or reduce parallel jobs
cargo +nightly build --release -j 2
```

**Linker Errors:**
```bash
# Install build dependencies
# Linux
sudo apt-get install build-essential pkg-config libssl-dev

# macOS
xcode-select --install
```

**Version Mismatch:**
```bash
# Update Rust toolchain
rustup update nightly
rustup default nightly
```

## Related Topics

- [Installation Guide](./INSTALLATION.md) - General installation
- [Development Guide](../../../CONTRIBUTING.md) - Contributing to Nexus
- [Architecture Guide](../../ARCHITECTURE.md) - System architecture

