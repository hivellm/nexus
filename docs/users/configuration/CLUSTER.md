---
title: Cluster Configuration
module: configuration
id: cluster-configuration
order: 6
description: Cluster setup and configuration
tags: [cluster, configuration, ha, high-availability]
---

# Cluster Configuration

Complete guide for configuring Nexus in cluster mode.

## Overview

Nexus supports master-replica clustering for high availability and read scaling.

## Cluster Architecture

```
┌─────────────┐
│   Master    │
│  (Writes)   │
└──────┬──────┘
       │ Replication
       │
┌──────▼──────┐  ┌─────────────┐
│   Replica   │  │   Replica   │
│  (Reads)    │  │  (Reads)    │
└─────────────┘  └─────────────┘
```

## Master Configuration

```yaml
server:
  bind_addr: "0.0.0.0:15474"

replication:
  role: "master"
  bind_addr: "0.0.0.0:15475"
  mode: "async"  # or "sync"
  sync_quorum: 1
```

## Replica Configuration

```yaml
server:
  bind_addr: "0.0.0.0:15474"

replication:
  role: "replica"
  master_addr: "192.168.1.100:15475"
  mode: "async"
  auto_failover: true
  heartbeat_ms: 5000
```

## Environment Variables

### Master

```bash
export NEXUS_REPLICATION_ROLE="master"
export NEXUS_REPLICATION_BIND_ADDR="0.0.0.0:15475"
export NEXUS_REPLICATION_MODE="async"
```

### Replica

```bash
export NEXUS_REPLICATION_ROLE="replica"
export NEXUS_REPLICATION_MASTER_ADDR="192.168.1.100:15475"
export NEXUS_REPLICATION_MODE="async"
export NEXUS_REPLICATION_AUTO_FAILOVER="true"
```

## Replication Modes

### Async Mode

- **Pros**: High performance, low latency
- **Cons**: Possible data loss on master failure
- **Use Case**: High-throughput scenarios

### Sync Mode

- **Pros**: Data durability, consistency
- **Cons**: Higher latency
- **Use Case**: Critical data scenarios

## Health Monitoring

### Heartbeat Configuration

```yaml
replication:
  heartbeat_ms: 5000
  connect_timeout_ms: 5000
  max_log_size: 1000000
```

### Automatic Failover

```yaml
replication:
  auto_failover: true
  failover_timeout_ms: 10000
```

## Related Topics

- [Replication Guide](../guides/REPLICATION.md) - Complete replication guide
- [Replication API](../api/REPLICATION.md) - API endpoints
- [Configuration Overview](./CONFIGURATION.md) - General configuration

