# Nexus Replication Guide

## Overview

Nexus supports master-replica replication for high availability and read scaling. The replication system streams WAL (Write-Ahead Log) entries from the master to replicas in real-time, ensuring data consistency across nodes.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                     Master Node                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │ WAL Stream  │  │  Replica    │  │  Snapshot   │  │
│  │   Sender    │  │  Tracker    │  │  Creator    │  │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  │
│         │                │                │          │
│         └────────────────┴────────────────┘          │
│                          │                            │
│                    TCP (15475)                        │
└──────────────────────────┬──────────────────────────┘
                           │
                           │ WAL Entries / Snapshots
                           │
┌──────────────────────────┴──────────────────────────┐
│                     Replica Node                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │ WAL Entry   │  │  Health     │  │  Snapshot   │  │
│  │  Applier    │  │  Monitor    │  │  Restorer   │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  │
└─────────────────────────────────────────────────────┘
```

## Features

- **WAL Streaming**: Real-time replication of write operations
- **Async/Sync Modes**: Choose between performance (async) and durability (sync with quorum)
- **Full Sync**: Automatic snapshot transfer for new replicas
- **Health Monitoring**: Heartbeat-based health checks with configurable thresholds
- **Automatic Failover**: Replica promotion when master becomes unavailable
- **CRC32 Validation**: All messages are validated for integrity

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `NEXUS_REPLICATION_ROLE` | Node role: `master`, `replica`, or `standalone` | `standalone` |
| `NEXUS_REPLICATION_BIND_ADDR` | Master bind address | `0.0.0.0:15475` |
| `NEXUS_REPLICATION_MASTER_ADDR` | Master address (for replicas) | - |
| `NEXUS_REPLICATION_MODE` | Replication mode: `async` or `sync` | `async` |
| `NEXUS_REPLICATION_SYNC_QUORUM` | Number of replicas for sync quorum | `1` |
| `NEXUS_REPLICATION_HEARTBEAT_MS` | Heartbeat interval in milliseconds | `5000` |
| `NEXUS_REPLICATION_CONNECT_TIMEOUT_MS` | Connection timeout | `5000` |
| `NEXUS_REPLICATION_MAX_LOG_SIZE` | Maximum replication log entries | `1000000` |
| `NEXUS_REPLICATION_AUTO_FAILOVER` | Enable automatic failover | `true` |

### Starting a Master Node

```bash
# Start Nexus as a replication master
NEXUS_REPLICATION_ROLE=master \
NEXUS_REPLICATION_BIND_ADDR=0.0.0.0:15475 \
NEXUS_REPLICATION_MODE=async \
./nexus-server
```

### Starting a Replica Node

```bash
# Start Nexus as a replica
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=192.168.1.100:15475 \
./nexus-server
```

## API Endpoints

### Get Replication Status

```bash
curl http://localhost:15474/replication/status
```

Response:
```json
{
  "role": "master",
  "running": true,
  "mode": "async",
  "connected": true,
  "node_id": "550e8400-e29b-41d4-a716-446655440000",
  "wal_offset": 12345,
  "replica_count": 2
}
```

### Get Master Statistics

```bash
curl http://localhost:15474/replication/master/stats
```

Response:
```json
{
  "entries_replicated": 100000,
  "bytes_sent": 52428800,
  "connected_replicas": 2,
  "healthy_replicas": 2,
  "log_size": 50000,
  "current_offset": 100000,
  "sync_acks": 95000,
  "snapshot_transfers": 1
}
```

### Get Replica Statistics

```bash
curl http://localhost:15474/replication/replica/stats
```

Response:
```json
{
  "entries_received": 99500,
  "entries_applied": 99500,
  "bytes_received": 51380224,
  "current_offset": 99500,
  "lag": 500,
  "reconnects": 0,
  "connected": true,
  "master_id": "550e8400-e29b-41d4-a716-446655440001"
}
```

### List Connected Replicas (Master Only)

```bash
curl http://localhost:15474/replication/replicas
```

Response:
```json
{
  "replicas": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440002",
      "addr": "192.168.1.10:15475",
      "last_ack_offset": 99000,
      "lag": 1000,
      "healthy": true,
      "connected_seconds": 3600
    }
  ]
}
```

### Promote Replica to Master

```bash
curl -X POST http://localhost:15474/replication/promote
```

Response:
```json
{
  "success": true,
  "message": "Replica promoted to master. Start master manually if needed."
}
```

### Create Snapshot

```bash
curl -X POST http://localhost:15474/replication/snapshot
```

Response:
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440003",
  "created_at": 1732900000000,
  "wal_offset": 100000,
  "uncompressed_size": 104857600,
  "compressed_size": 31457280,
  "files_count": 5
}
```

### Get Last Snapshot Info

```bash
curl http://localhost:15474/replication/snapshot
```

### Stop Replication

```bash
curl -X POST http://localhost:15474/replication/stop
```

## Wire Protocol

The replication protocol uses a binary format with CRC32 validation:

```
[message_type:1][length:4][payload:N][crc32:4]
```

### Message Types

| Code | Type | Description |
|------|------|-------------|
| 0x01 | Hello | Replica handshake (replica → master) |
| 0x02 | Welcome | Master response (master → replica) |
| 0x10 | Ping | Heartbeat ping |
| 0x11 | Pong | Heartbeat pong |
| 0x20 | WalEntry | WAL entry to replicate |
| 0x21 | WalAck | WAL entry acknowledgment |
| 0x30 | RequestSnapshot | Request full sync |
| 0x31 | SnapshotMeta | Snapshot metadata |
| 0x32 | SnapshotChunk | Snapshot data chunk |
| 0x33 | SnapshotComplete | Snapshot transfer complete |
| 0xFF | Error | Error message |

## Replication Modes

### Async Replication (Default)

- Master doesn't wait for replica acknowledgments
- Lower latency for writes
- Potential for data loss if master fails before replication

```bash
NEXUS_REPLICATION_MODE=async
```

### Sync Replication

- Master waits for quorum acknowledgments before confirming write
- Higher durability guarantees
- Higher write latency

```bash
NEXUS_REPLICATION_MODE=sync
NEXUS_REPLICATION_SYNC_QUORUM=1  # Wait for at least 1 replica
```

## Full Sync (Snapshot Transfer)

When a new replica connects or falls too far behind, a full sync is initiated:

1. Master creates a compressed snapshot (tar + zstd)
2. Snapshot is transferred in chunks with CRC32 validation
3. Replica restores the snapshot
4. Incremental WAL streaming resumes

### Snapshot Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `NEXUS_SNAPSHOT_COMPRESSION_LEVEL` | Zstd compression level (0-22) | `3` |
| `NEXUS_SNAPSHOT_MAX_SIZE` | Maximum snapshot size in bytes | `10GB` |
| `NEXUS_SNAPSHOT_CHUNK_SIZE` | Transfer chunk size | `1MB` |

## Failover

### Automatic Failover

When `NEXUS_REPLICATION_AUTO_FAILOVER=true`:

1. Replica monitors master via heartbeats (every 5 seconds)
2. After 3 missed heartbeats (15 seconds), master is considered dead
3. Replica can be promoted to master via API

### Manual Failover

```bash
# On the replica node
curl -X POST http://localhost:15474/replication/promote
```

### Recommended Failover Procedure

1. Ensure the old master is stopped
2. Promote the most up-to-date replica:
   ```bash
   curl -X POST http://replica:15474/replication/promote
   ```
3. Reconfigure other replicas to point to the new master
4. Restart replicas with new master address

## Monitoring

### Key Metrics to Monitor

- **Replication Lag**: `lag` in replica stats (should be near 0)
- **Connected Replicas**: `connected_replicas` in master stats
- **Healthy Replicas**: `healthy_replicas` in master stats
- **Reconnect Count**: `reconnects` in replica stats (should be low)

### Health Check Example

```bash
# Check master health
curl http://master:15474/replication/status | jq '.running and .replica_count > 0'

# Check replica health
curl http://replica:15474/replication/status | jq '.connected and .lag < 1000'
```

## Troubleshooting

### Replica Can't Connect

1. Check network connectivity: `telnet master-host 15475`
2. Verify master is running: `curl http://master:15474/replication/status`
3. Check firewall rules allow port 15475

### High Replication Lag

1. Check network bandwidth between master and replica
2. Increase `max_log_size` if replicas disconnect frequently
3. Consider reducing write load on master

### Snapshot Transfer Fails

1. Check disk space on replica
2. Verify snapshot size is within limits
3. Check network stability during transfer

### Master Appears Dead (False Positive)

1. Increase `missed_heartbeats_threshold`
2. Check network latency between nodes
3. Reduce `heartbeat_ms` interval

## Best Practices

1. **Use at least 2 replicas** for high availability
2. **Monitor replication lag** continuously
3. **Test failover procedures** regularly
4. **Use sync replication** for critical data
5. **Keep snapshots** for disaster recovery
6. **Place replicas in different availability zones**

## Limitations

- Single-master architecture (no multi-master)
- Replicas are read-only
- No automatic master election (manual promotion required)
- Runtime replication start not supported (configure at startup)
