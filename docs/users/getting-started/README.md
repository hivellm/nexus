---
title: Getting Started
module: getting-started
id: getting-started-index
order: 0
description: Installation and quick start guides
tags: [getting-started, installation, quick-start, tutorial]
---

# Getting Started

Complete guides to install Nexus and get started quickly.

## Installation Guides

### [Installation Guide](./INSTALLATION.md)
Quick installation and overview:
- Quick installation scripts (Linux/macOS, Windows)
- Manual installation
- Verification steps

### [Docker Installation](./DOCKER.md)
Complete Docker deployment guide:
- Docker Compose examples
- Volumes and networking
- Health checks and resource limits
- Backup and restore

### [Building from Source](./BUILD_FROM_SOURCE.md)
Build Nexus from source code:
- Prerequisites and dependencies
- Build process and optimization
- Feature flags and cross-compilation
- Development workflow

## Quick Start Guides

### [Quick Start Guide](./QUICK_START.md)
Get up and running in minutes:
- Execute your first Cypher query
- Create nodes and relationships
- Perform vector search
- Using SDKs

### [First Steps](./FIRST_STEPS.md)
Complete guide after installation:
- Verify installation
- Create first database
- Insert first data
- Perform first query
- Next steps

## Quick Installation

**Linux/macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.sh | bash
```

**Windows:**
```powershell
powershell -c "irm https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.ps1 | iex"
```

## Next Steps

After installation:
1. **[First Steps](./FIRST_STEPS.md)** - Verify and setup
2. **[Cypher Basics](../cypher/BASIC.md)** - Learn Cypher syntax
3. **[Vector Search](../vector-search/BASIC.md)** - Start vector search
4. **[Use Cases](../use-cases/)** - See examples

## Related Topics

- [Cypher Guide](../cypher/CYPHER.md) - Query language
- [API Reference](../api/API_REFERENCE.md) - REST API
- [SDKs Guide](../sdks/README.md) - Client SDKs

