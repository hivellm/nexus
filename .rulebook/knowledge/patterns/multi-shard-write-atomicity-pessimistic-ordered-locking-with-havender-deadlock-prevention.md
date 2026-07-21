# Multi-shard write atomicity: pessimistic ordered locking with Havender deadlock prevention

**Category**: distributed-systems
**Tags**: distributed, transactions, raft, cross-shard, deadlock-prevention, havender

## Description

For cross-shard atomic writes, pessimistic ordered locking ships before 2PC. Coordinator builds an ordered `WriteSet` (BTreeSet over `shard_id`s), acquires write leases in *strict ascending shard_id order*, mutates each shard while holding its lease, releases leases in reverse order on success, and on any failure rolls back every previously-mutated shard before releasing leases. Deadlock prevention is Havender total-order resource ordering — no two transactions can form a wait-for cycle. Per-call lock-acquire timeout is independent of the outer tx timeout so a slow shard cannot consume the whole budget. The public API hides the protocol so a future swap to full 2PC is forward-compatible. Trade vs 2PC: lower throughput under contention, but no coordinator-state recovery procedure (leases time out shard-side if the coordinator dies).

## Example

// Coordinator path
let write_set = WriteSet::from_iter([s(0), s(2), s(1)]); // dedup + ordered
let orch = MultiShardTx::new(&locks, &mutator, &metrics);
match orch.execute(tx_id, &write_set) {
    Ok(()) => /* committed */,
    Err(MultiShardTxError::Lock(LockError::Timeout { .. })) => /* retry */,
    Err(MultiShardTxError::Mutation { shard, .. }) => /* surface to client */,
    Err(MultiShardTxError::TxTimeout { .. }) => /* retry whole tx */,
}
// Acquisition iterates iter_ordered() — guaranteed ascending shard_id.
// On any error, unwind() walks `mutated` in REVERSE order to roll back,
// then releases every acquired lease. Rollback failures are logged but
// do NOT mask the original error.

## When to Use

Distributed engines that need atomic multi-shard writes without a coordinator-state recovery procedure. Useful when (a) deadlock-freedom must be provable from the protocol, (b) the coordinator cannot be assumed to be reliably restartable mid-commit, (c) contention is bounded (most workloads have hot single shards more often than hot shard pairs).

## When NOT to Use

Workloads with frequent multi-shard write contention on overlapping shard sets — the head-of-line blocking from ordered acquisition becomes the bottleneck and you should ship 2PC instead. Also avoid for read-only transactions; the locking is unnecessary overhead there.
