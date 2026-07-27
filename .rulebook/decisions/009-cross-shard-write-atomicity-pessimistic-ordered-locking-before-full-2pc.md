# 9. Cross-shard write atomicity: pessimistic ordered locking before full 2PC

**Status**: proposed
**Date**: 2026-04-29
**Related Tasks**: phase8_cross-shard-2pc

## Context

V2 cluster mode (Phase 5/6) ships hash-partitioned shards with per-shard Raft consensus, a metadata Raft group, scatter-gather queries, cross-shard reads. The single missing piece for production-grade multi-shard production deployments is atomic multi-shard writes — the coordinator currently `fail-atomics` any mutation whose write set spans more than one shard. Two options solve this: (a) cross-shard 2-phase commit with prepare/commit log entries replicated by each shard's Raft group, (b) pessimistic ordered locking — the coordinator acquires write leases on every shard in ascending `shard_id` order, executes the per-shard mutation, releases. Phase 8 must pick one.

## Decision

Ship pessimistic ordered locking now. Layer 2PC on top in a phase-9 follow-up if pessimistic-locking throughput becomes a contention bottleneck under workloads with many simultaneous multi-shard writers. The public API (`MultiShardTx::execute(tx, write_set)`) hides which protocol is in use, so the swap is forward-compatible.

## Alternatives Considered

- Cross-shard 2PC with prepare/commit log entries on every shard's Raft group — higher throughput in low-contention workloads but requires a coordinator-state recovery procedure (the coordinator may crash between prepare and commit, leaving shards in `prepared` state). The recovery story alone is multiple weeks of work and an additional failure mode to test.
- Optimistic locking with conflict detection at commit — high throughput on no-conflict, but a hot-shard workload thrashes; needs application-level retry semantics every other engine in this space avoids.

## Consequences

Pros: zero coordinator recovery story (leases time out on the shard side if the coordinator dies), classical Havender deadlock prevention via total shard-id ordering (no cycle is possible), small surface to test (~700 LOC + 11 unit tests cover the chaos cases), forward-compatible with a future 2PC swap. Cons: lower throughput under contention because each lease is held for the duration of the mutation rather than just the prepare; tail latency proportional to slowest shard. Mitigation: per-call lock-acquire timeout independent of the outer tx timeout so a slow shard cannot consume the entire budget.
