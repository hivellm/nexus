# Proposal: phase8_cross-shard-2pc

## Why

V2 cluster mode (sharding + per-shard Raft + coordinator scatter-gather) is functionally complete for **single-shard writes only**. Multi-shard mutations are explicitly unsupported — the coordinator fail-atomic when a query touches multiple shards. This is the structural gating issue for advertising V2 as "production cluster mode" rather than "production single-node + read-fanout." Memgraph HA, ArangoDB cluster, Dgraph, NebulaGraph all support multi-shard writes; Nexus must too. Two paths exist: cross-shard 2PC (proper distributed transactions) or pessimistic per-shard locking with deterministic-order acquisition (simpler, lower-throughput). Recommendation: ship pessimistic locking first, layer 2PC on top in a follow-up.

## What Changes

- Implement pessimistic-order shard locking: any multi-shard write acquires shard locks in `shard_id` ascending order, executes the per-shard mutation, and releases on commit/abort.
- Implement timeout + abort path: if a lock cannot be acquired within `tx_timeout_ms`, abort the whole transaction and surface a clear error.
- Implement deterministic deadlock-prevention: ordered acquisition guarantees no cycle is possible.
- Add a coordinator-side "write set" tracker per transaction: which shards have mutations.
- Add `nexus_cluster_multi_shard_writes_total` + `_aborted_total` metrics.
- Add chaos-test cases: leader churn mid-multi-shard-write, network partition mid-write, slow-disk on one shard.
- Document the contract: read-consistency across shards within a transaction is now snapshot-isolated; outside a transaction, eventual.
- Spec the follow-up (phase 9) for full Paxos-style 2PC if pessimistic-locking throughput becomes a bottleneck.

## Impact

- Affected specs: new `docs/specs/cluster-transactions.md`, update `docs/CLUSTER_MODE.md`.
- Affected code: `crates/nexus-core/src/coordinator/`, `crates/nexus-core/src/sharding/raft/`, transaction layer.
- Breaking change: NO (currently rejected → now executes).
- User benefit: V2 cluster mode advertisable for production multi-shard writes; closes the largest V2 correctness gap.
