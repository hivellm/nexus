---
title: Configuration
module: configuration
id: configuration-index
order: 0
description: Complete configuration guides
tags: [configuration, setup, server, logging]
---

# Configuration

Complete configuration guides for Nexus.

## Guides

### [Configuration Overview](./CONFIGURATION.md)

Quick reference and overview:
- Configuration methods
- Environment variables
- Config files
- Priority order

### [Server Configuration](./SERVER.md)

Network and server settings:
- Network binding
- Ports and host binding
- Reverse proxy setup
- SSL/TLS configuration

### [Logging Configuration](./LOGGING.md)

Log management:
- Log levels
- Log filtering
- Log aggregation
- Log rotation

### [Data Directory](./DATA_DIRECTORY.md)

Storage configuration:
- Storage paths
- Data migration
- Backup locations
- Disk space management

### [Performance Tuning](./PERFORMANCE_TUNING.md)

Performance optimization:
- Thread configuration
- Memory settings
- Cache configuration
- Query optimization

## Quick Reference

### Environment Variables

```bash
# Server
export NEXUS_BIND_ADDR="0.0.0.0:15474"

# Authentication
export NEXUS_ROOT_USERNAME="admin"
export NEXUS_ROOT_PASSWORD="secure_password"
export NEXUS_AUTH_ENABLED="true"

# Data
export NEXUS_DATA_DIR="./data"
```

### Config File

Create `config.yml`:

```yaml
server:
  bind_addr: "0.0.0.0:15474"
  data_dir: "./data"

auth:
  enabled: true
  root_user:
    username: "admin"
    password: "secure_password"
```

## Related Topics

- [Installation Guide](../getting-started/INSTALLATION.md) - Installation instructions
- [Operations Guide](../operations/) - Service management
- [Performance Guide](../guides/PERFORMANCE.md) - Performance optimization

