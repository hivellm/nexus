# Act - Running GitHub Actions Locally

This document explains how to use `act` to run GitHub Actions workflows locally, simulating the CI/CD environment.

## What is act?

`act` is a CLI that reads workflows in `.github/workflows/*.yml` and executes them locally inside Docker containers, simulating the GitHub Actions environment.

## Installation

### On WSL (Ubuntu)

```bash
# Method 1: Installation script
curl -s https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash

# Method 2: Direct download
cd /mnt/f/Node/hivellm/nexus
curl -sL https://github.com/nektos/act/releases/latest/download/act_Linux_x86_64.tar.gz | tar -xz
chmod +x act
```

### On Windows (via WSL)

The PowerShell script `scripts/act-run-workflows.ps1` automatically downloads `act` if needed.

## Prerequisites

1. **Docker Desktop** must be running
2. **Docker Desktop** must expose daemon via TCP:
   - Open Docker Desktop
   - Go to Settings > General
   - Check "Expose daemon on tcp://localhost:2375"
   - Make sure "Use TLS" is **unchecked** (for local use)
   - Click "Apply & Restart"

**Note:** If Docker only works in PowerShell (not in WSL), this TCP configuration is required for `act` to connect from WSL to Windows Docker.

## Usage

### List available workflows

```bash
# On WSL
./scripts/act-run-workflows.sh

# On PowerShell
.\scripts\act-run-workflows.ps1
```

### Run a specific job

```bash
# On WSL
./scripts/act-run-workflows.sh rust-tests
./scripts/act-run-workflows.sh lint
./scripts/act-run-workflows.sh codespell

# On PowerShell
.\scripts\act-run-workflows.ps1 rust-tests
.\scripts\act-run-workflows.ps1 lint
.\scripts\act-run-workflows.ps1 codespell
```

### Run all jobs

```bash
# On WSL
./scripts/act-run-workflows.sh all

# On PowerShell
.\scripts\act-run-workflows.ps1 all
```

### Direct act usage

```bash
# List workflows
./act -l

# Run a specific job
./act -j rust-tests \
    --container-architecture linux/amd64 \
    --image ubuntu-latest=ghcr.io/catthehacker/ubuntu:act-latest

# Run an event (e.g., push)
./act push \
    --container-architecture linux/amd64 \
    --image ubuntu-latest=ghcr.io/catthehacker/ubuntu:act-latest
```

## Available Workflows

1. **rust-tests** - Runs all Rust tests
2. **lint** - Checks formatting and clippy
3. **codespell** - Checks spelling errors

## Docker Images

The script uses the `ghcr.io/catthehacker/ubuntu:act-latest` image which is equivalent to GitHub Actions' `ubuntu-latest`.

## Troubleshooting

### Docker not accessible

If you get "Docker not accessible" error:

1. Verify Docker Desktop is running
2. In Docker Desktop, go to Settings > General
3. Enable "Expose daemon on tcp://localhost:2375"
4. Make sure "Use TLS" is unchecked
5. Click "Apply & Restart"

**For WSL integration (alternative):**
1. In Docker Desktop, go to Settings > Resources > WSL Integration
2. Enable integration with Ubuntu-24.04
3. Restart WSL: `wsl --shutdown` and then open again

### Permission error

```bash
chmod +x act
chmod +x scripts/act-run-workflows.sh
```

### Docker cache

act may use Docker cache. To force rebuild:

```bash
./act -j rust-tests --pull
```

## Advantages

- ✅ Test workflows before pushing
- ✅ Faster debugging (no waiting for CI)
- ✅ Saves GitHub Actions resources
- ✅ Identical environment to CI

## Limitations

- Some GitHub actions may not work locally
- GitHub secrets are not available (use environment variables)
- Jobs running on `windows-latest` or `macos-latest` only work on Linux via Docker
