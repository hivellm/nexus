---
title: Installation Guide
module: installation
id: installation-guide
order: 1
description: Complete guide for installing Nexus on Linux, macOS, and Windows
tags: [installation, setup, linux, windows, macos]
---

# Installation Guide

This guide covers installing Nexus Graph Database on different platforms.

## Quick Installation

### Linux/macOS

```bash
curl -fsSL https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.sh | bash
```

This will:
- ✅ Install Nexus server to `/usr/local/bin`
- ✅ Configure as systemd service (Linux)
- ✅ Start service automatically
- ✅ Enable auto-start on boot

### Windows

```powershell
powershell -c "irm https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.ps1 | iex"
```

**Note:** Service installation requires Administrator privileges.

This will:
- ✅ Install Nexus server to `%USERPROFILE%\.cargo\bin`
- ✅ Configure as Windows Service
- ✅ Start service automatically
- ✅ Enable auto-start on boot

## Installation Methods

- **[Docker Installation](./DOCKER.md)** - Complete Docker deployment guide
- **[Building from Source](./BUILD_FROM_SOURCE.md)** - Build Nexus from source code

### Quick Docker Installation

```bash
docker run -d \
  --name nexus \
  -p 15474:15474 \
  -v $(pwd)/nexus-data:/app/data \
  --restart unless-stopped \
  ghcr.io/hivellm/nexus:latest
```

### Quick Build from Source

```bash
git clone https://github.com/hivellm/nexus.git
cd nexus
cargo +nightly build --release --workspace
```

The binary will be at `target/release/nexus-server` (or `target/release/nexus-server.exe` on Windows).

## Prerequisites

- **Rust**: Nightly 1.85+ (edition 2024)
- **RAM**: 8GB+ recommended
- **OS**: Linux, macOS, or Windows with WSL

## Verification

After installation, verify the installation:

```bash
# Check service status (Linux)
sudo systemctl status nexus

# Check service status (Windows)
Get-Service Nexus

# Test API endpoint
curl http://localhost:15474/health
```

Expected health check response:
```json
{
  "status": "healthy",
  "version": "0.12.0",
  "uptime_seconds": 123
}
```

## Configuration

### Environment Variables

Key configuration options:

```bash
# Server binding
export NEXUS_BIND_ADDR="0.0.0.0:15474"

# Root user (change in production!)
export NEXUS_ROOT_USERNAME="admin"
export NEXUS_ROOT_PASSWORD="secure_password_here"
export NEXUS_ROOT_ENABLED="true"
export NEXUS_DISABLE_ROOT_AFTER_SETUP="true"

# Authentication
export NEXUS_AUTH_ENABLED="true"

# Data directory
export NEXUS_DATA_DIR="./data"
```

## Access Points

After installation, Nexus is available at:

- 🌐 **REST API**: `http://localhost:15474` (StreamableHTTP with SSE)
- 🔌 **MCP Server**: `http://localhost:15474/mcp`
- 🔗 **UMICP**: `http://localhost:15474/umicp`
- 🔍 **Tool Discovery**: `http://localhost:15474/umicp/discover`
- ❤️ **Health Check**: `http://localhost:15474/health`
- 📊 **Statistics**: `http://localhost:15474/stats`

## Related Topics

- [Service Management](../operations/SERVICE_MANAGEMENT.md) - Managing the Nexus service
- [Quick Start Guide](./QUICK_START.md) - Next steps after installation
- [Configuration Guide](../configuration/CONFIGURATION.md) - Configuring Nexus

