---
title: Replication
module: guides
id: replication-guide
order: 5
description: Master-replica setup
tags: [replication, ha, high-availability, master-replica]
---

# Replication

Complete guide for master-replica replication in Nexus.

## Overview

Nexus supports Redis-style master-replica replication for:
- **High Availability**: Automatic failover
- **Read Scaling**: Distribute read load
- **Data Redundancy**: Multiple copies of data

## Architecture

```
┌─────────────┐
│   Master    │
│  (Writes)   │
└──────┬──────┘
       │ WAL Stream
       │
┌──────▼──────┐  ┌─────────────┐
│   Replica   │  │   Replica   │
│  (Reads)    │  │  (Reads)    │
└─────────────┘  └─────────────┘
```

## Configuration

### Master Configuration

```bash
export NEXUS_REPLICATION_ROLE="master"
export NEXUS_REPLICATION_BIND_ADDR="0.0.0.0:15475"
export NEXUS_REPLICATION_MODE="async"
```

### Replica Configuration

```bash
export NEXUS_REPLICATION_ROLE="replica"
export NEXUS_REPLICATION_MASTER_ADDR="192.168.1.100:15475"
export NEXUS_REPLICATION_MODE="async"
```

## Setup

### Start Master

```bash
NEXUS_REPLICATION_ROLE=master \
NEXUS_REPLICATION_BIND_ADDR=0.0.0.0:15475 \
./nexus-server
```

### Start Replica

```bash
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=192.168.1.100:15475 \
./nexus-server
```

## Replication Modes

### Async Mode

```bash
export NEXUS_REPLICATION_MODE="async"
```

- **Pros**: High performance, low latency
- **Cons**: Possible data loss on master failure

### Sync Mode

```bash
export NEXUS_REPLICATION_MODE="sync"
export NEXUS_REPLICATION_SYNC_QUORUM=1
```

- **Pros**: Data durability
- **Cons**: Higher latency

## Failover

### Automatic Failover

```bash
export NEXUS_REPLICATION_AUTO_FAILOVER="true"
export NEXUS_REPLICATION_HEARTBEAT_MS=5000
```

When master becomes unavailable, replicas automatically promote.

### Manual Failover

```bash
POST /replication/promote
```

## Monitoring

### Replication Status

```bash
GET /replication/status
```

**Response:**
```json
{
  "role": "master",
  "connected_replicas": 2,
  "replication_lag_ms": 10
}
```

## Related Topics

- [Replication API](../api/REPLICATION.md) - API endpoints
- [Operations Guide](../operations/MONITORING.md) - Monitoring
- [Configuration Guide](../configuration/CONFIGURATION.md) - Server configuration

