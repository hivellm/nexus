---
title: Logging Configuration
module: configuration
id: logging-configuration
order: 3
description: Log levels, filtering, aggregation
tags: [logging, logs, configuration]
---

# Logging Configuration

Complete guide for configuring Nexus logging.

## Log Levels

### Available Levels

- **TRACE**: Very detailed debugging information
- **DEBUG**: Debugging information
- **INFO**: General information (default)
- **WARN**: Warning messages
- **ERROR**: Error messages

### Set Log Level

**Environment Variable:**
```bash
export RUST_LOG=debug
```

**Config File:**
```yaml
logging:
  level: "debug"
```

**Module-Specific:**
```bash
export RUST_LOG=nexus_core=debug,nexus_server=info
```

## Log Outputs

### File Output

```yaml
logging:
  level: "info"
  file:
    enabled: true
    path: "/var/log/nexus/nexus.log"
    max_size_mb: 100
    max_files: 10
```

### Console Output

```yaml
logging:
  level: "info"
  console:
    enabled: true
    format: "json"  # or "text"
```

### Syslog Output

```yaml
logging:
  level: "info"
  syslog:
    enabled: true
    address: "logs.example.com:514"
    facility: "local0"
```

## Log Format

### JSON Format

```yaml
logging:
  format: "json"
```

**Output:**
```json
{
  "timestamp": "2025-01-01T10:00:00Z",
  "level": "INFO",
  "message": "Server started",
  "module": "nexus_server"
}
```

### Text Format

```yaml
logging:
  format: "text"
```

**Output:**
```
2025-01-01 10:00:00 INFO [nexus_server] Server started
```

## Log Filtering

### Filter by Module

```bash
# Only nexus_server logs
export RUST_LOG=nexus_server=info

# Multiple modules
export RUST_LOG=nexus_server=info,nexus_core=debug
```

### Filter by Level

```yaml
logging:
  level: "warn"  # Only warnings and errors
```

## Log Rotation

### Automatic Rotation

```yaml
logging:
  file:
    enabled: true
    path: "/var/log/nexus/nexus.log"
    rotation:
      max_size_mb: 100
      max_files: 10
      compress: true
```

### Manual Rotation (logrotate)

Create `/etc/logrotate.d/nexus`:

```
/var/log/nexus/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 0644 nexus nexus
}
```

## Log Aggregation

### Send to Centralized Logging

```yaml
logging:
  outputs:
    - type: "file"
      path: "/var/log/nexus/nexus.log"
    - type: "syslog"
      address: "logs.example.com:514"
    - type: "http"
      url: "https://logs.example.com/api/logs"
      headers:
        Authorization: "Bearer token"
```

## Query Logging

### Log All Queries

```yaml
logging:
  query_logging:
    enabled: true
    log_all: true
```

### Log Slow Queries

```yaml
logging:
  query_logging:
    enabled: true
    slow_query_threshold_ms: 1000
```

## Related Topics

- [Configuration Overview](./CONFIGURATION.md) - General configuration
- [Server Configuration](./SERVER.md) - Network settings
- [Log Management](../operations/LOGS.md) - Viewing and analyzing logs

