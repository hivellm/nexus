---
title: Log Management
module: operations
id: log-management
order: 2
description: Viewing, filtering, and analyzing logs
tags: [logs, logging, debugging, troubleshooting]
---

# Log Management

Complete guide for viewing, filtering, and analyzing Nexus logs.

## Default log levels

Nexus's default filter is deliberately quiet — production logs carry
`WARN` and above only, so per-query flavour text never shows up in
the default stream. The server's `tracing_subscriber::EnvFilter`
resolves in this order:

1. `RUST_LOG` environment variable (if set).
2. `--verbose` flag on `nexus-server` — enables a development-grade
   default: `nexus_server=debug,nexus_core=debug,tower_http=debug,hnsw_rs=warn`.
3. No env, no flag — production default:
   `nexus_server=error,nexus_core=warn,tower_http=error,hnsw_rs=warn`.

The `hnsw_rs=warn` floor is load-bearing: without it, the upstream
KNN crate emits an `INFO` line per index access (`Hnsw
max_nb_connection 16 … entering PointIndexation drop`) that in a
previous incident accounted for **95 %** of a production log
stream, masking actual errors.

## Re-enabling deep tracing

Every hot-path log line inside `executor/` was downgraded to `trace!`
in `phase4_tracing-hotpath-hygiene`, and three core entry points —
`Executor::execute`, `execute_create_with_context`, and
`execute_expand` — now carry `#[tracing::instrument(skip_all,
level = "debug")]` spans. That means callers get:

- **Default:** zero per-query log lines. Warnings and errors only.
- **`--verbose` (or `RUST_LOG=nexus_core=debug`):** one span per
  `execute` call with the Cypher source as a field, plus one nested
  span per `CREATE` and per `EXPAND`. Enough to answer "which
  query was slow" without drowning in per-row chatter.
- **`RUST_LOG=nexus_core=trace`:** the full per-row detail the
  operators had before the cleanup — every input row, every
  relationship skip, every node-id lookup. Use only while actively
  debugging a specific query.

Example — turn on operator-level tracing without touching the
server config:

```bash
RUST_LOG=nexus_core=debug,hnsw_rs=warn \
  ./target/release/nexus-server
```

Example — full per-row detail (heavy output; scope to a short
window):

```bash
RUST_LOG=nexus_core=trace,hnsw_rs=warn \
  ./target/release/nexus-server 2>&1 | grep -v 'EXPAND\|CREATE'
```

The filter additions for `hnsw_rs=warn` are retained in every mode
so the KNN firehose stays muzzled.

## Log Locations

### Linux

- **systemd**: `journalctl -u nexus`
- **File**: `/var/log/nexus/nexus.log` (if configured)

### Windows

- **Service Logs**: `C:\ProgramData\Nexus\logs\nexus.log`

### Docker

```bash
# View logs
docker logs nexus

# Follow logs
docker logs -f nexus

# Last 100 lines
docker logs --tail 100 nexus
```

## Viewing Logs

### Linux (systemd)

```bash
# View all logs
sudo journalctl -u nexus

# Follow logs
sudo journalctl -u nexus -f

# Last 100 lines
sudo journalctl -u nexus -n 100

# Since today
sudo journalctl -u nexus --since today

# Since specific time
sudo journalctl -u nexus --since "2025-01-01 10:00:00"
```

### Windows

```powershell
# View logs
Get-Content C:\ProgramData\Nexus\logs\nexus.log

# Follow logs
Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100 -Wait

# Last 100 lines
Get-Content C:\ProgramData\Nexus\logs\nexus.log -Tail 100
```

## Filtering Logs

### By Log Level

```bash
# Errors only
sudo journalctl -u nexus -p err

# Warnings and above
sudo journalctl -u nexus -p warning

# Info and above
sudo journalctl -u nexus -p info
```

### By Time Range

```bash
# Last hour
sudo journalctl -u nexus --since "1 hour ago"

# Last day
sudo journalctl -u nexus --since "1 day ago"

# Specific date range
sudo journalctl -u nexus --since "2025-01-01" --until "2025-01-02"
```

### By Content

```bash
# Search for specific text
sudo journalctl -u nexus | grep "ERROR"

# Case insensitive
sudo journalctl -u nexus | grep -i "error"

# Multiple patterns
sudo journalctl -u nexus | grep -E "ERROR|WARN"
```

## Log Levels

### Available Levels

- **TRACE**: Very detailed debugging information
- **DEBUG**: Debugging information
- **INFO**: General information (default)
- **WARN**: Warning messages
- **ERROR**: Error messages

### Setting Log Level

```bash
# Environment variable
export RUST_LOG=debug

# Config file
logging:
  level: "debug"
```

## Log Rotation

### Linux (logrotate)

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

### Windows

Use Windows Event Log or configure log rotation in application.

## Analyzing Logs

### Common Patterns

```bash
# Count errors
sudo journalctl -u nexus -p err | wc -l

# Find most common errors
sudo journalctl -u nexus -p err | grep -oP 'ERROR: \K.*' | sort | uniq -c | sort -rn

# Query performance
sudo journalctl -u nexus | grep "execution_time_ms" | awk '{print $NF}' | sort -n
```

### Log Analysis Tools

```bash
# Using jq for JSON logs
sudo journalctl -u nexus -o json | jq 'select(.level == "ERROR")'

# Using awk for parsing
sudo journalctl -u nexus | awk '/ERROR/ {print $0}'
```

## Debugging

### Enable Debug Logging

```bash
# Set debug level
export RUST_LOG=debug

# Restart service
sudo systemctl restart nexus

# View debug logs
sudo journalctl -u nexus -f
```

### Query Debugging

```cypher
// Use EXPLAIN to see query plan
EXPLAIN MATCH (n:Person) RETURN n

// Use PROFILE to see execution stats
PROFILE MATCH (n:Person) RETURN n
```

## Log Aggregation

### Send to Centralized Logging

```yaml
# Configure log forwarding
logging:
  level: "info"
  outputs:
    - type: "file"
      path: "/var/log/nexus/nexus.log"
    - type: "syslog"
      address: "logs.example.com:514"
```

## Related Topics

- [Service Management](./SERVICE_MANAGEMENT.md) - Managing services
- [Monitoring](./MONITORING.md) - Health checks and metrics
- [Troubleshooting](./TROUBLESHOOTING.md) - Common problems

