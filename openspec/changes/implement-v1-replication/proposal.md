# Implement V1 Replication System

## Why

Read scalability and fault tolerance require master-replica replication. Following Redis/Vectorizer approach with async replication for performance and optional sync for durability.

## What Changes

- Implement master-replica architecture
- Implement WAL streaming to replicas
- Implement full sync (snapshot transfer) and incremental sync
- Implement failover support (health monitoring + replica promotion)
- Add replication management API endpoints

**BREAKING**: None (opt-in feature)

## Impact

### Affected Specs
- NEW capability: `replication`
- NEW capability: `failover`

### Affected Code
- `nexus-core/src/replication/master.rs` - Master node (~400 lines)
- `nexus-core/src/replication/replica.rs` - Replica node (~400 lines)
- `nexus-server/src/api/replication.rs` - Replication API (~200 lines)
- `tests/replication_tests.rs` - Replication tests (~500 lines)

### Dependencies
- Requires: MVP complete + authentication

### Timeline
- **Duration**: 2 weeks
- **Complexity**: High

