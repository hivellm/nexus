---
title: Configuration Overview
module: configuration
id: configuration-overview
order: 1
description: Quick reference and overview
tags: [configuration, setup, reference]
---

# Configuration Overview

Quick reference and overview of Nexus configuration.

## Configuration Methods

Nexus supports multiple configuration methods with priority order:

1. **Environment Variables** (highest priority)
2. **Config File** (`config.yml` or `config/auth.toml`)
3. **Default Values** (lowest priority)

## Environment Variables

### Server Configuration

```bash
# Network binding
export NEXUS_BIND_ADDR="0.0.0.0:15474"

# Data directory
export NEXUS_DATA_DIR="./data"

# Log level
export RUST_LOG="info"
```

### Authentication Configuration

```bash
# Root user
export NEXUS_ROOT_USERNAME="admin"
export NEXUS_ROOT_PASSWORD="secure_password"
export NEXUS_ROOT_ENABLED="true"
export NEXUS_DISABLE_ROOT_AFTER_SETUP="true"

# Authentication
export NEXUS_AUTH_ENABLED="true"
export NEXUS_AUTH_REQUIRED_FOR_PUBLIC="true"
```

### Performance Configuration

```bash
# Thread pool
export NEXUS_THREAD_POOL_SIZE="4"

# Cache size
export NEXUS_CACHE_SIZE_MB="1024"

# Connection pool
export NEXUS_MAX_CONNECTIONS="100"
```

## Config File

### YAML Format (`config.yml`)

```yaml
server:
  bind_addr: "0.0.0.0:15474"
  data_dir: "./data"
  max_connections: 100

auth:
  enabled: true
  required_for_public: true
  root_user:
    username: "admin"
    password: "secure_password"
    enabled: true
    disable_after_setup: true

cache:
  max_size_mb: 1024
  eviction_policy: "lru"

logging:
  level: "info"
  file: "./logs/nexus.log"
```

### TOML Format (`config/auth.toml`)

```toml
[root_user]
username = "admin"
password = "secure_password"
enabled = true
disable_after_setup = true
```

## Configuration Priority

When multiple configuration sources are present:

1. Environment variables override config files
2. Config files override defaults
3. Defaults are used if nothing is specified

## Common Configurations

### Development

```yaml
server:
  bind_addr: "127.0.0.1:15474"

auth:
  enabled: false

logging:
  level: "debug"
```

### Production

```yaml
server:
  bind_addr: "0.0.0.0:15474"
  max_connections: 1000

auth:
  enabled: true
  required_for_public: true
  root_user:
    disable_after_setup: true

cache:
  max_size_mb: 4096

logging:
  level: "info"
  file: "/var/log/nexus/nexus.log"
```

## Related Topics

- [Server Configuration](./SERVER.md) - Network and server settings
- [Logging Configuration](./LOGGING.md) - Log management
- [Performance Tuning](./PERFORMANCE_TUNING.md) - Performance optimization

