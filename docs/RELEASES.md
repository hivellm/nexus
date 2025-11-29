# Nexus Release Process

This document describes how to create releases for Nexus CLI and Server.

## Overview

Nexus uses GitHub Actions workflows to automatically build and publish release artifacts for multiple platforms:
- **Linux**: x86_64 GNU, x86_64 MUSL (static), ARM64 MUSL
- **macOS**: Intel (x86_64), Apple Silicon (ARM64)
- **Windows**: x86_64

## Release Workflows

### 1. Nexus CLI Release (`.github/workflows/release-cli.yml`)

Builds and publishes the `nexus` command-line interface.

**Artifacts produced:**
- Binary archives for all platforms
- Debian packages (`.deb`) for Linux x86_64 and ARM64
- Man pages included in Linux packages

**Trigger:**
- Publish a GitHub release
- Manual workflow dispatch with tag input

**Usage:**
```bash
# Create a release tag
git tag cli-v0.12.0
git push origin cli-v0.12.0

# Or create a GitHub release via UI with tag like "cli-v0.12.0"
```

### 2. Nexus Server Release (`.github/workflows/release-server.yml`)

Builds and publishes the `nexus-server` database server.

**Artifacts produced:**
- Binary archives for all platforms
- Debian packages (`.deb`) with systemd service
- Docker images pushed to GitHub Container Registry

**Trigger:**
- Publish a GitHub release
- Manual workflow dispatch with tag input

**Usage:**
```bash
# Create a release tag
git tag server-v0.11.0
git push origin server-v0.11.0

# Or create a GitHub release via UI with tag like "server-v0.11.0"
```

## Debian Packages

Both CLI and Server include full Debian package support with:

### CLI Package Features
- Binary installed to `/usr/bin/nexus`
- Man pages in `/usr/share/man/man1/`
- Documentation in `/usr/share/doc/nexus-cli/`
- Auto-dependency resolution

### Server Package Features
- Binary installed to `/usr/bin/nexus-server`
- Configuration in `/etc/nexus/`
- Data directory: `/var/lib/nexus/`
- Log directory: `/var/log/nexus/`
- Systemd service: `nexus-server.service`
- Automatic user/group creation (`nexus`)
- Auto-dependency resolution

**Install Debian package:**
```bash
# Download .deb from GitHub Releases
sudo dpkg -i nexus-server_*.deb

# Start service
sudo systemctl start nexus-server
sudo systemctl enable nexus-server

# Check status
sudo systemctl status nexus-server
```

## Docker Images (Server Only)

Server releases automatically build and push multi-architecture Docker images.

**Registry:** `ghcr.io/hivellm/nexus-server`

**Tags:**
- `latest` - Latest release
- `X.Y.Z` - Specific version (e.g., `0.11.0`)

**Usage:**
```bash
# Pull latest
docker pull ghcr.io/hivellm/nexus-server:latest

# Pull specific version
docker pull ghcr.io/hivellm/nexus-server:0.11.0

# Run
docker run -p 15474:15474 ghcr.io/hivellm/nexus-server:latest
```

**Platforms supported:**
- `linux/amd64`
- `linux/arm64`

## Systemd Service (Server Debian Package)

The server Debian package includes a complete systemd service configuration.

**Service file:** `/lib/systemd/system/nexus-server.service`

**Features:**
- Automatic restart on failure
- Security hardening (NoNewPrivileges, PrivateTmp, ProtectSystem)
- Resource limits (65536 file descriptors, 4096 processes)
- Journal logging with identifier

**Configuration:**
```bash
# Edit configuration
sudo nano /etc/nexus/config.yml

# Reload systemd
sudo systemctl daemon-reload

# Restart service
sudo systemctl restart nexus-server

# View logs
sudo journalctl -u nexus-server -f
```

## Creating a New Release

### CLI Release

1. **Update version** in `nexus-cli/Cargo.toml` and `Cargo.lock`:
   ```bash
   cd nexus-cli
   # Edit version in Cargo.toml
   cargo build --release  # Updates Cargo.lock
   ```

2. **Update CHANGELOG** in `nexus-cli/CHANGELOG.md`:
   - Add new version section
   - List all changes

3. **Commit changes**:
   ```bash
   git add nexus-cli/Cargo.toml Cargo.lock nexus-cli/CHANGELOG.md
   git commit -m "chore: bump CLI version to 0.12.0"
   git push
   ```

4. **Create and push tag**:
   ```bash
   git tag cli-v0.12.0
   git push origin cli-v0.12.0
   ```

5. **Create GitHub release**:
   - Go to GitHub → Releases → New Release
   - Tag: `cli-v0.12.0`
   - Title: `Nexus CLI v0.12.0`
   - Description: Copy from CHANGELOG
   - Publish release

6. **Wait for workflows**: Check Actions tab for build status

### Server Release

1. **Update version** in `nexus-server/Cargo.toml` and `Cargo.lock`:
   ```bash
   cd nexus-server
   # Edit version in Cargo.toml
   cargo build --release  # Updates Cargo.lock
   ```

2. **Update CHANGELOG** (if exists or create one)

3. **Commit changes**:
   ```bash
   git add nexus-server/Cargo.toml Cargo.lock
   git commit -m "chore: bump server version to 0.11.0"
   git push
   ```

4. **Create and push tag**:
   ```bash
   git tag server-v0.11.0
   git push origin server-v0.11.0
   ```

5. **Create GitHub release** (same as CLI)

6. **Verify Docker image**:
   ```bash
   docker pull ghcr.io/hivellm/nexus-server:0.11.0
   docker images | grep nexus-server
   ```

## Manual Workflow Dispatch

You can also trigger releases manually without creating tags:

1. Go to GitHub → Actions
2. Select workflow (release-cli or release-server)
3. Click "Run workflow"
4. Enter tag (e.g., `cli-v0.12.0` or `server-v0.11.0`)
5. Click "Run workflow"

## Build Matrix

### CLI Platforms

| Platform | Target | Package |
|----------|--------|---------|
| Linux x86_64 (GNU) | x86_64-unknown-linux-gnu | tar.gz |
| Linux x86_64 (MUSL) | x86_64-unknown-linux-musl | tar.gz, .deb |
| Linux ARM64 (MUSL) | aarch64-unknown-linux-musl | tar.gz |
| macOS x86_64 | x86_64-apple-darwin | tar.gz |
| macOS ARM64 | aarch64-apple-darwin | tar.gz |
| Windows x86_64 | x86_64-pc-windows-msvc | zip |

### Server Platforms

Same as CLI, plus:
- Docker multi-arch images (amd64, arm64)
- Systemd service in Debian packages

## Troubleshooting

### Build Failures

**Linux MUSL cross-compilation fails:**
- Check `cross` installation
- Verify target is added: `rustup target add x86_64-unknown-linux-musl`

**Debian package creation fails:**
- Ensure `cargo-deb` is installed in workflow
- Check `[package.metadata.deb]` in Cargo.toml
- Verify all asset paths exist

**Docker push fails:**
- Check GitHub token permissions
- Verify `contents: write` permission in workflow

### Testing Locally

**Build Linux MUSL binary:**
```bash
cargo install cross
cross build --release --target x86_64-unknown-linux-musl
```

**Build Debian package:**
```bash
cargo install cargo-deb
cargo deb --target x86_64-unknown-linux-musl
```

**Build Docker image:**
```bash
docker build -t nexus-server:test .
```

## Release Checklist

### Before Release

- [ ] All tests passing
- [ ] Version bumped in Cargo.toml
- [ ] CHANGELOG updated
- [ ] README updated (if needed)
- [ ] Git tag created and pushed
- [ ] GitHub release drafted

### After Release

- [ ] Verify all workflow jobs completed successfully
- [ ] Download and test binaries for major platforms
- [ ] Test Debian package installation
- [ ] Test Docker image
- [ ] Update documentation links
- [ ] Announce release

## Related Documentation

- [Vectorizer Release Process](../../vectorizer/.github/workflows/release-artifacts.yml) - Reference implementation
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [cargo-deb](https://github.com/kornelski/cargo-deb)
- [cross](https://github.com/cross-rs/cross)
- [taiki-e/upload-rust-binary-action](https://github.com/taiki-e/upload-rust-binary-action)
