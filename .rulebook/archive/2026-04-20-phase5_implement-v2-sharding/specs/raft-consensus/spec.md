# Raft Consensus Spec

## ADDED Requirements

### Requirement: Per-Shard Raft Group

Each data shard SHALL be a Raft group with `replica_factor` members (default
3). The metadata shard SHALL be a separate Raft group with the same member
set as the cluster's founding nodes.

#### Scenario: Leader is the sole writer
Given a 3-replica shard with leader L and followers F1, F2
When a write is accepted at L and committed
Then F1 and F2 SHALL apply the write through the Raft log apply loop
And no other writer SHALL touch the shard's storage layer

### Requirement: Log Replication

The system MUST replicate every write as a Raft log entry committed by a
majority quorum before acknowledging the client.

#### Scenario: Minority failure tolerated
Given a 3-replica shard
When 1 replica crashes during a write
Then the write SHALL still commit (majority = 2)
And the client SHALL observe the write on any surviving replica

#### Scenario: Majority failure blocks writes
Given a 3-replica shard
When 2 replicas crash
Then writes SHALL block until a majority is restored
And reads that tolerate stale data MAY still succeed on the surviving replica

### Requirement: Leader Election

The system SHALL elect a new leader within 3 × election timeout after the
previous leader stops sending heartbeats.

#### Scenario: Fast failover
Given a shard with `election_timeout = 500-1000ms` and leader L
When L is killed via SIGKILL
Then a new leader SHALL be elected within 3 seconds
And no committed log entries SHALL be lost

### Requirement: Snapshot Transfer

The system SHALL support installing a snapshot on a new or lagging replica
using the existing `replication::Snapshot` zstd+tar format.

#### Scenario: Bootstrapping a new replica
Given a running 2-replica shard at log offset 1,000,000
When a third replica joins
Then the leader SHALL stream a snapshot to the new replica
And the new replica SHALL resume log replication from the snapshot's
  last-included index within 60 seconds for a 1 GB dataset

### Requirement: Log Compaction

The system SHALL trigger a snapshot and truncate the log when the number of
log entries since the last snapshot exceeds `snapshot_log_size_threshold`
(default 10,000).

#### Scenario: Log growth bound
Given a shard under sustained 10k writes/sec
When 1 hour of sustained writes has elapsed
Then the log on disk SHALL NOT exceed 2 × `snapshot_log_size_threshold`
  entries (allowing one in-flight snapshot to complete)

### Requirement: Wire Format

All Raft RPCs SHALL use the wire format:
```
[shard_id:u32][message_type:u8][length:u32][payload:N][crc32:u32]
```
payload is bincode-encoded openraft message.

#### Scenario: Corrupted frame rejected
Given a Raft RPC received with a CRC mismatch
When the receiver validates the frame
Then it SHALL drop the frame and log the mismatch
And it SHALL NOT apply the message
