---
title: Replication API
module: api
id: replication-api
order: 6
description: Master-replica replication
tags: [replication, ha, high-availability, api]
---

# Replication API

Complete guide for master-replica replication in Nexus.

## Overview

Nexus supports Redis-style master-replica replication for high availability and read scaling.

## Replication Status

### Get Replication Status

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

## Master Operations

### Start Replication

```bash
POST /replication/start
```

### Stop Replication

```bash
POST /replication/stop
```

## Replica Operations

### Connect to Master

```bash
POST /replication/connect
Content-Type: application/json

{
  "master_address": "192.168.1.100:15475"
}
```

### Disconnect from Master

```bash
POST /replication/disconnect
```

## Snapshots

### Create Snapshot

```bash
POST /replication/snapshots
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "created_at": 1700000000,
  "wal_offset": 100000
}
```

### List Snapshots

```bash
GET /replication/snapshots
```

## Failover

### Promote Replica

```bash
POST /replication/promote
```

Promotes a replica to master when the current master is unavailable.

## Related Topics

- [Replication Guide](../guides/REPLICATION.md) - Complete replication guide
- [API Reference](./API_REFERENCE.md) - REST API documentation
- [Operations Guide](../operations/) - Service management

