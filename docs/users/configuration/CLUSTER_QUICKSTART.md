---
title: Cluster Quickstart
module: configuration
id: cluster-quickstart
order: 7
description: Quick cluster setup guide
tags: [cluster, quickstart, ha, high-availability]
---

# Cluster Quickstart

Quick guide to set up a Nexus cluster.

## Prerequisites

- 2+ servers (1 master, 1+ replicas)
- Network connectivity between servers
- Same Nexus version on all nodes

## Quick Setup

### 1. Start Master

```bash
NEXUS_REPLICATION_ROLE=master \
NEXUS_REPLICATION_BIND_ADDR=0.0.0.0:15475 \
./nexus-server
```

### 2. Start Replica

```bash
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=192.168.1.100:15475 \
./nexus-server
```

### 3. Verify Cluster

```bash
# Check master status
curl http://localhost:15474/replication/status

# Check replica status
curl http://replica-ip:15474/replication/status
```

## Docker Compose Example

```yaml
version: '3.8'

services:
  master:
    image: ghcr.io/hivellm/nexus:latest
    environment:
      - NEXUS_REPLICATION_ROLE=master
      - NEXUS_REPLICATION_BIND_ADDR=0.0.0.0:15475
    ports:
      - "15474:15474"
      - "15475:15475"

  replica1:
    image: ghcr.io/hivellm/nexus:latest
    environment:
      - NEXUS_REPLICATION_ROLE=replica
      - NEXUS_REPLICATION_MASTER_ADDR=master:15475
    depends_on:
      - master
```

## Next Steps

- [Cluster Configuration](./CLUSTER.md) - Complete cluster guide
- [Replication Guide](../guides/REPLICATION.md) - Replication details
- [Monitoring Guide](../operations/MONITORING.md) - Cluster monitoring

