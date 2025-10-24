# Implementation Tasks - V1 Replication

## 1. Master Node Implementation

- [ ] 1.1 Implement WAL streaming to replicas
- [ ] 1.2 Track connected replicas
- [ ] 1.3 Implement async replication (don't wait for ACK)
- [ ] 1.4 Implement sync replication (wait for quorum ACK)
- [ ] 1.5 Implement circular replication log (1M operations)
- [ ] 1.6 Add replica health monitoring
- [ ] 1.7 Add unit tests (95%+ coverage)

## 2. Replica Node Implementation

- [ ] 2.1 Connect to master via TCP
- [ ] 2.2 Receive and apply WAL entries
- [ ] 2.3 Validate CRC32 on received entries
- [ ] 2.4 Send ACK to master (for sync replication)
- [ ] 2.5 Implement auto-reconnect (exponential backoff)
- [ ] 2.6 Track replication lag
- [ ] 2.7 Add unit tests (95%+ coverage)

## 3. Full Sync (Snapshot Transfer)

- [ ] 3.1 Create snapshot (tar.zst archive)
- [ ] 3.2 Calculate CRC32 checksum
- [ ] 3.3 Transfer snapshot to replica
- [ ] 3.4 Verify checksum on replica
- [ ] 3.5 Load snapshot into replica storage
- [ ] 3.6 Switch to incremental sync
- [ ] 3.7 Add integration tests

## 4. Failover Support

- [ ] 4.1 Implement health check endpoint
- [ ] 4.2 Implement heartbeat monitoring (every 5s)
- [ ] 4.3 Detect master failure (3 missed heartbeats)
- [ ] 4.4 Implement replica promotion (POST /replication/promote)
- [ ] 4.5 Update catalog role (master/replica)
- [ ] 4.6 Add failover integration tests

## 5. Replication API

- [ ] 5.1 GET /replication/status
- [ ] 5.2 POST /replication/promote
- [ ] 5.3 POST /replication/pause
- [ ] 5.4 POST /replication/resume
- [ ] 5.5 GET /replication/lag
- [ ] 5.6 Add API tests

## 6. Documentation & Quality

- [ ] 6.1 Update docs/ROADMAP.md
- [ ] 6.2 Add replication guide to README
- [ ] 6.3 Update CHANGELOG.md with v0.6.0
- [ ] 6.4 Run all quality checks

