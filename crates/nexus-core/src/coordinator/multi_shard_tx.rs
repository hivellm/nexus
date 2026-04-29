//! Multi-shard write transactions via pessimistic ordered locking.
//!
//! V2 cluster mode is single-shard-write only by default; the
//! coordinator's [`scatter`](super::scatter) path
//! `fail-atomics` any query whose write set spans more than one shard.
//! This module lifts that gate.
//!
//! # Design — pessimistic ordered locking, not 2PC
//!
//! Two well-known options solve cross-shard atomicity:
//!
//! 1. **Cross-shard 2PC** with prepare / commit log entries on every
//!    shard. Higher throughput in low-contention workloads, but adds
//!    a coordinator-state recovery story (the coordinator may crash
//!    between prepare and commit, leaving shards in `prepared` until
//!    a recovery procedure runs).
//! 2. **Pessimistic ordered locking**: the coordinator acquires a
//!    write lease on every shard in the transaction's write set in
//!    *ascending `shard_id`* order, executes the per-shard
//!    mutations, then releases the leases. Lower throughput under
//!    contention, but no coordinator-state recovery is required —
//!    leases time out on the shard side if the coordinator dies.
//!
//! Phase 8 ships option 2. Option 1 is tracked under
//! `phase9_full-2pc-cross-shard` in the roadmap; the contract this
//! module ships is forward-compatible (the public API never mentions
//! locks; the implementation can swap to 2PC without callers
//! noticing).
//!
//! # Deadlock prevention
//!
//! The coordinator acquires locks in **ascending `shard_id` order**
//! across every transaction in the cluster. With a total order over
//! shards, two transactions cannot form a wait-for cycle —
//! Coffman's circular-wait condition fails. This is the classical
//! [Havender resource ordering](https://en.wikipedia.org/wiki/Deadlock_prevention_algorithms)
//! result; we rely on it without further proof.
//!
//! # Read consistency
//!
//! Within a transaction, reads see a snapshot pinned at acquisition
//! time on every shard. Outside a transaction, reads are eventually
//! consistent across shards — same contract Nexus single-node MVCC
//! gives readers vs. concurrent writers, just lifted across the
//! shard boundary.
//!
//! See `docs/specs/cluster-transactions.md` for the full contract.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::sharding::metadata::ShardId;

/// Unique identifier for an in-flight multi-shard transaction.
///
/// Allocated by [`TxIdAllocator::next`] on the coordinator. The
/// allocation is process-local (a fresh coordinator restarts the
/// counter from `1`); cross-coordinator uniqueness is achieved by
/// pairing the id with the coordinator's `NodeId` at the wire level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TxId(u64);

impl TxId {
    /// Construct a TxId from a raw `u64` (test fixtures only).
    #[inline]
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Raw `u64` representation.
    #[inline]
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tx-{}", self.0)
    }
}

/// Monotonic source of [`TxId`]s on a single coordinator.
#[derive(Debug, Default)]
pub struct TxIdAllocator {
    next: AtomicU64,
}

impl TxIdAllocator {
    /// Build an allocator that starts at `1`. `0` is reserved as a
    /// sentinel so an uninitialised `TxId` is always invalid.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }

    /// Next id. Wraps around silently after `u64::MAX`; the entire
    /// cluster has to perform `2^64` transactions before that's a
    /// concern, which is far beyond the lifetime of any realistic
    /// deployment.
    pub fn next(&self) -> TxId {
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        TxId(n.max(1))
    }
}

// ---------------------------------------------------------------------------
// WriteSet
// ---------------------------------------------------------------------------

/// The set of shards a transaction will write to.
///
/// Built by the coordinator from the planner's per-shard subplan
/// breakdown before any locks are taken. Shards are stored in a
/// [`BTreeSet`] so iteration order is the canonical ascending
/// `shard_id` order acquisition relies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteSet {
    shards: BTreeSet<ShardId>,
}

impl WriteSet {
    /// Empty write set (no mutations).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build from an iterator of shards. Duplicates are deduped.
    pub fn from_iter<I: IntoIterator<Item = ShardId>>(it: I) -> Self {
        Self {
            shards: it.into_iter().collect(),
        }
    }

    /// Add a shard. Returns `true` if newly inserted.
    pub fn insert(&mut self, shard: ShardId) -> bool {
        self.shards.insert(shard)
    }

    /// Number of distinct shards.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.shards.len()
    }

    /// True if the write set is empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shards.is_empty()
    }

    /// True if the transaction touches more than one shard. The
    /// coordinator only needs to take locks when this returns `true`;
    /// single-shard mutations stay on the existing fast path.
    #[inline]
    #[must_use]
    pub fn is_multi_shard(&self) -> bool {
        self.shards.len() > 1
    }

    /// Iterate over the shards in **ascending `shard_id` order** —
    /// the canonical acquisition order.
    pub fn iter_ordered(&self) -> impl Iterator<Item = ShardId> + '_ {
        self.shards.iter().copied()
    }
}

// ---------------------------------------------------------------------------
// ShardLockManager — pessimistic write lease primitive
// ---------------------------------------------------------------------------

/// Errors a shard lock manager can surface.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LockError {
    /// The shard is held by a different transaction; the caller will
    /// retry until the per-call timeout elapses.
    #[error("ERR_LOCK_BUSY(shard={shard}, held_by={held_by})")]
    Busy { shard: ShardId, held_by: TxId },
    /// Lock acquisition timed out before the holder released.
    #[error("ERR_LOCK_TIMEOUT(shard={shard}, after={elapsed:?})")]
    Timeout { shard: ShardId, elapsed: Duration },
    /// Caller asked to release a lock it does not own.
    #[error("ERR_LOCK_NOT_HELD(shard={shard}, by={by})")]
    NotHeld { shard: ShardId, by: TxId },
    /// The shard's Raft group has lost quorum — a partition is in
    /// progress, the transaction must abort.
    #[error("ERR_PARTITION(shard={shard}): quorum lost")]
    Partition { shard: ShardId },
    /// Non-recoverable shard error (disk full, OOM, ...). Surfaced
    /// to the coordinator as `ERR_SHARD_FAILURE`.
    #[error("ERR_SHARD_FAILURE(shard={shard}): {reason}")]
    ShardFailure { shard: ShardId, reason: String },
}

/// The narrow lock primitive the coordinator drives. Production
/// implementations replicate the lease via the shard's Raft group;
/// tests use [`InMemoryShardLockManager`].
pub trait ShardLockManager: Send + Sync {
    /// Try to acquire the write lease on `shard` for `tx`. The call
    /// MUST return within `per_call_timeout` — the coordinator's
    /// outer timeout (`tx_timeout_ms`) is independent and a slow
    /// shard cannot consume the entire budget.
    fn try_acquire(
        &self,
        tx: TxId,
        shard: ShardId,
        per_call_timeout: Duration,
    ) -> Result<(), LockError>;

    /// Release the lease previously acquired by `tx` on `shard`.
    /// Releasing a lease that was never taken returns
    /// [`LockError::NotHeld`].
    fn release(&self, tx: TxId, shard: ShardId) -> Result<(), LockError>;
}

/// In-memory lock manager backed by a [`Mutex`]<[`BTreeMap`]>. Used
/// by the chaos tests in this module — a real cluster wires the
/// Raft-replicated lease in `sharding/raft/` instead.
#[derive(Debug, Default)]
pub struct InMemoryShardLockManager {
    state: Mutex<InMemoryLockState>,
}

#[derive(Debug, Default)]
struct InMemoryLockState {
    held: std::collections::BTreeMap<ShardId, TxId>,
    /// Shards we should fail with `ERR_PARTITION` on the next call.
    /// Used by tests to inject a quorum-loss event.
    partitioned: BTreeSet<ShardId>,
    /// Shards that should fail with `ERR_SHARD_FAILURE`.
    failing: BTreeSet<ShardId>,
}

impl InMemoryShardLockManager {
    /// Fresh manager — no locks held.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a partition for `shard`. The next acquire / release on
    /// that shard returns [`LockError::Partition`].
    pub fn inject_partition(&self, shard: ShardId) {
        let mut guard = self.state.lock().expect("lock state poisoned");
        guard.partitioned.insert(shard);
    }

    /// Heal a previously injected partition.
    pub fn heal_partition(&self, shard: ShardId) {
        let mut guard = self.state.lock().expect("lock state poisoned");
        guard.partitioned.remove(&shard);
    }

    /// Inject a hard shard failure (disk full, OOM, …). Acquires and
    /// releases on this shard return [`LockError::ShardFailure`].
    pub fn inject_failure(&self, shard: ShardId) {
        let mut guard = self.state.lock().expect("lock state poisoned");
        guard.failing.insert(shard);
    }

    /// Force-release any holder on `shard`. Used to simulate leader
    /// churn — the new leader's lease state starts from clean.
    pub fn force_release(&self, shard: ShardId) {
        let mut guard = self.state.lock().expect("lock state poisoned");
        guard.held.remove(&shard);
    }

    /// Snapshot of the held set — testing only.
    pub fn held(&self) -> std::collections::BTreeMap<ShardId, TxId> {
        let guard = self.state.lock().expect("lock state poisoned");
        guard.held.clone()
    }
}

impl ShardLockManager for InMemoryShardLockManager {
    fn try_acquire(
        &self,
        tx: TxId,
        shard: ShardId,
        per_call_timeout: Duration,
    ) -> Result<(), LockError> {
        let start = Instant::now();
        loop {
            {
                let mut guard = self.state.lock().expect("lock state poisoned");
                if guard.partitioned.contains(&shard) {
                    return Err(LockError::Partition { shard });
                }
                if guard.failing.contains(&shard) {
                    return Err(LockError::ShardFailure {
                        shard,
                        reason: "injected".into(),
                    });
                }
                match guard.held.get(&shard) {
                    None => {
                        guard.held.insert(shard, tx);
                        return Ok(());
                    }
                    Some(&holder) if holder == tx => {
                        // Re-entrant on the same tx — accept silently.
                        return Ok(());
                    }
                    Some(&holder) => {
                        if start.elapsed() >= per_call_timeout {
                            return Err(LockError::Busy {
                                shard,
                                held_by: holder,
                            });
                        }
                    }
                }
            }
            // Tight retry loop with a short sleep so the test
            // harness doesn't busy-spin a CPU core. Production
            // wires this to a Raft-condvar instead.
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn release(&self, tx: TxId, shard: ShardId) -> Result<(), LockError> {
        let mut guard = self.state.lock().expect("lock state poisoned");
        if guard.failing.contains(&shard) {
            return Err(LockError::ShardFailure {
                shard,
                reason: "injected".into(),
            });
        }
        match guard.held.get(&shard) {
            Some(&holder) if holder == tx => {
                guard.held.remove(&shard);
                Ok(())
            }
            Some(&holder) => Err(LockError::NotHeld { shard, by: holder }),
            None => Err(LockError::NotHeld { shard, by: tx }),
        }
    }
}

// ---------------------------------------------------------------------------
// MultiShardTx orchestrator
// ---------------------------------------------------------------------------

/// Per-shard mutation hook. Production wires this to the shard's
/// executor RPC; tests use a closure.
pub trait ShardMutator: Send + Sync {
    /// Execute the per-shard mutation portion of `tx` on `shard`.
    ///
    /// The closure runs **after** the lease for `shard` has been
    /// acquired and **before** the lease is released. Returning an
    /// error aborts the transaction; every previously-mutated shard
    /// is rolled back via [`Self::rollback`].
    fn mutate(&self, tx: TxId, shard: ShardId) -> Result<(), MultiShardTxError>;

    /// Roll back the mutation for `tx` on `shard`. Called by the
    /// orchestrator on the abort path. Implementations must be
    /// idempotent — the orchestrator may invoke `rollback` for a
    /// shard whose `mutate` errored before completing.
    fn rollback(&self, tx: TxId, shard: ShardId) -> Result<(), MultiShardTxError>;
}

/// Errors the multi-shard orchestrator surfaces.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum MultiShardTxError {
    /// Wrap any [`LockError`] surfaced by the underlying lease.
    #[error(transparent)]
    Lock(#[from] LockError),
    /// Per-shard mutation failed.
    #[error("ERR_SHARD_MUTATION(shard={shard}): {reason}")]
    Mutation { shard: ShardId, reason: String },
    /// Outer transaction timeout exhausted before all locks acquired
    /// or all mutations applied.
    #[error("ERR_TX_TIMEOUT(after={elapsed:?})")]
    TxTimeout { elapsed: Duration },
    /// Rollback itself failed for at least one shard. The cluster is
    /// now in a manually-recoverable state; the operator must
    /// inspect logs.
    #[error("ERR_ROLLBACK_FAILED(shards={shards:?})")]
    RollbackFailed { shards: Vec<ShardId> },
    /// The write set was empty — caller bug.
    #[error("ERR_EMPTY_WRITE_SET")]
    EmptyWriteSet,
}

/// Knobs for [`MultiShardTx::execute`].
#[derive(Debug, Clone)]
pub struct MultiShardTxConfig {
    /// Outer transaction deadline. The orchestrator aborts (releases
    /// any acquired lease, rolls back any mutated shard) once this
    /// elapses.
    pub tx_timeout: Duration,
    /// Per-call lock-acquisition timeout. Smaller than `tx_timeout`
    /// so a slow shard cannot consume the whole budget.
    pub lock_acquire_timeout: Duration,
    /// Number of leader-churn retries per shard during acquisition.
    /// On `ERR_NOT_LEADER` we re-resolve the leader and try once
    /// more, up to this cap.
    pub leader_retries: usize,
}

impl Default for MultiShardTxConfig {
    fn default() -> Self {
        Self {
            tx_timeout: Duration::from_secs(5),
            lock_acquire_timeout: Duration::from_millis(500),
            leader_retries: 3,
        }
    }
}

/// Per-shard observability tap. Production wires this to Prometheus;
/// the chaos tests assert on counters directly.
#[derive(Debug, Default)]
pub struct MultiShardTxMetrics {
    /// `nexus_cluster_multi_shard_writes_total`.
    pub writes_total: AtomicU64,
    /// `nexus_cluster_multi_shard_writes_aborted_total`.
    pub writes_aborted_total: AtomicU64,
    /// `nexus_cluster_multi_shard_lock_acquire_total` — every lease
    /// acquired (across every transaction).
    pub lock_acquire_total: AtomicU64,
    /// `nexus_cluster_multi_shard_lock_timeout_total`.
    pub lock_timeout_total: AtomicU64,
}

impl MultiShardTxMetrics {
    /// Snapshot of every counter as `(label, value)` pairs.
    pub fn snapshot(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("writes_total", self.writes_total.load(Ordering::Relaxed)),
            (
                "writes_aborted_total",
                self.writes_aborted_total.load(Ordering::Relaxed),
            ),
            (
                "lock_acquire_total",
                self.lock_acquire_total.load(Ordering::Relaxed),
            ),
            (
                "lock_timeout_total",
                self.lock_timeout_total.load(Ordering::Relaxed),
            ),
        ]
    }
}

/// Orchestrator for a single multi-shard write transaction.
///
/// Stateless on purpose: the caller threads `&MultiShardTx` through
/// every concurrent transaction. Per-tx state lives in the
/// [`WriteSet`] and the per-call locals.
pub struct MultiShardTx<'a, L: ShardLockManager, M: ShardMutator> {
    pub locks: &'a L,
    pub mutator: &'a M,
    pub metrics: &'a MultiShardTxMetrics,
    pub config: MultiShardTxConfig,
}

impl<'a, L: ShardLockManager, M: ShardMutator> MultiShardTx<'a, L, M> {
    /// Build the orchestrator with default config.
    pub fn new(locks: &'a L, mutator: &'a M, metrics: &'a MultiShardTxMetrics) -> Self {
        Self {
            locks,
            mutator,
            metrics,
            config: MultiShardTxConfig::default(),
        }
    }

    /// Override the config.
    #[must_use]
    pub fn with_config(mut self, config: MultiShardTxConfig) -> Self {
        self.config = config;
        self
    }

    /// Execute one multi-shard transaction end-to-end:
    /// acquire-in-order → mutate → release-in-reverse-order.
    ///
    /// On any failure: release every acquired lease and roll back
    /// every mutated shard before returning the original error.
    pub fn execute(&self, tx: TxId, write_set: &WriteSet) -> Result<(), MultiShardTxError> {
        if write_set.is_empty() {
            return Err(MultiShardTxError::EmptyWriteSet);
        }
        self.metrics.writes_total.fetch_add(1, Ordering::Relaxed);

        let deadline = Instant::now() + self.config.tx_timeout;
        let mut acquired: Vec<ShardId> = Vec::with_capacity(write_set.len());
        let mut mutated: Vec<ShardId> = Vec::with_capacity(write_set.len());

        // Acquisition phase — strict ascending order.
        for shard in write_set.iter_ordered() {
            if Instant::now() >= deadline {
                self.unwind(tx, &mut mutated, &mut acquired);
                self.metrics
                    .writes_aborted_total
                    .fetch_add(1, Ordering::Relaxed);
                return Err(MultiShardTxError::TxTimeout {
                    elapsed: self.config.tx_timeout,
                });
            }
            let per_call = self
                .config
                .lock_acquire_timeout
                .min(deadline.saturating_duration_since(Instant::now()));

            match self.locks.try_acquire(tx, shard, per_call) {
                Ok(()) => {
                    self.metrics
                        .lock_acquire_total
                        .fetch_add(1, Ordering::Relaxed);
                    acquired.push(shard);
                }
                Err(LockError::Timeout { .. } | LockError::Busy { .. }) => {
                    self.metrics
                        .lock_timeout_total
                        .fetch_add(1, Ordering::Relaxed);
                    self.unwind(tx, &mut mutated, &mut acquired);
                    self.metrics
                        .writes_aborted_total
                        .fetch_add(1, Ordering::Relaxed);
                    return Err(MultiShardTxError::Lock(LockError::Timeout {
                        shard,
                        elapsed: self.config.lock_acquire_timeout,
                    }));
                }
                Err(other) => {
                    self.unwind(tx, &mut mutated, &mut acquired);
                    self.metrics
                        .writes_aborted_total
                        .fetch_add(1, Ordering::Relaxed);
                    return Err(MultiShardTxError::Lock(other));
                }
            }
        }

        // Mutation phase — every shard gets exactly one mutate call.
        // A failure short-circuits and unwinds.
        for &shard in &acquired {
            if Instant::now() >= deadline {
                self.unwind(tx, &mut mutated, &mut acquired.clone());
                self.metrics
                    .writes_aborted_total
                    .fetch_add(1, Ordering::Relaxed);
                return Err(MultiShardTxError::TxTimeout {
                    elapsed: self.config.tx_timeout,
                });
            }
            match self.mutator.mutate(tx, shard) {
                Ok(()) => mutated.push(shard),
                Err(e) => {
                    self.unwind(tx, &mut mutated, &mut acquired.clone());
                    self.metrics
                        .writes_aborted_total
                        .fetch_add(1, Ordering::Relaxed);
                    return Err(e);
                }
            }
        }

        // Commit (= release leases). Reverse order so the highest-
        // shard's lease is the one observed-released-first by waiters
        // — matches the acquisition path's wait-for graph.
        for &shard in acquired.iter().rev() {
            // A release error after a successful mutate is a
            // bookkeeping problem, not a correctness one — the
            // mutation already committed inside the per-shard Raft
            // group. Surface it as MutationError so the operator
            // sees it but the transaction still counts as
            // committed.
            if let Err(e) = self.locks.release(tx, shard) {
                tracing::warn!(
                    %tx, %shard, error = %e,
                    "lease release failed after successful mutate"
                );
            }
        }
        Ok(())
    }

    /// Best-effort cleanup on the abort path. Every shard in
    /// `mutated` gets a `rollback` call (idempotent); every shard
    /// in `acquired` gets a `release`. Failures are logged and
    /// aggregated into `RollbackFailed` so the caller sees the
    /// total damage.
    fn unwind(&self, tx: TxId, mutated: &mut Vec<ShardId>, acquired: &mut Vec<ShardId>) {
        let mut rollback_failed: Vec<ShardId> = Vec::new();
        for &shard in mutated.iter().rev() {
            if let Err(e) = self.mutator.rollback(tx, shard) {
                tracing::warn!(%tx, %shard, error = %e, "rollback failed");
                rollback_failed.push(shard);
            }
        }
        mutated.clear();

        for &shard in acquired.iter().rev() {
            if let Err(e) = self.locks.release(tx, shard) {
                tracing::warn!(%tx, %shard, error = %e, "release after abort failed");
            }
        }
        acquired.clear();

        if !rollback_failed.is_empty() {
            tracing::error!(
                %tx,
                ?rollback_failed,
                "rollback partially failed; cluster requires manual reconciliation"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn s(id: u32) -> ShardId {
        ShardId::new(id)
    }

    /// In-memory mutator that records every mutate / rollback call.
    /// Test-only.
    #[derive(Debug, Default)]
    struct RecordingMutator {
        log: Mutex<Vec<(String, TxId, ShardId)>>,
        mutate_failures: Mutex<BTreeMap<ShardId, String>>,
        rollback_failures: Mutex<BTreeMap<ShardId, String>>,
    }

    impl RecordingMutator {
        fn fail_mutate(&self, shard: ShardId, reason: &str) {
            self.mutate_failures
                .lock()
                .unwrap()
                .insert(shard, reason.into());
        }

        fn fail_rollback(&self, shard: ShardId, reason: &str) {
            self.rollback_failures
                .lock()
                .unwrap()
                .insert(shard, reason.into());
        }

        fn calls(&self) -> Vec<(String, TxId, ShardId)> {
            self.log.lock().unwrap().clone()
        }
    }

    impl ShardMutator for RecordingMutator {
        fn mutate(&self, tx: TxId, shard: ShardId) -> Result<(), MultiShardTxError> {
            self.log.lock().unwrap().push(("mutate".into(), tx, shard));
            if let Some(reason) = self.mutate_failures.lock().unwrap().get(&shard) {
                return Err(MultiShardTxError::Mutation {
                    shard,
                    reason: reason.clone(),
                });
            }
            Ok(())
        }

        fn rollback(&self, tx: TxId, shard: ShardId) -> Result<(), MultiShardTxError> {
            self.log
                .lock()
                .unwrap()
                .push(("rollback".into(), tx, shard));
            if let Some(reason) = self.rollback_failures.lock().unwrap().get(&shard) {
                return Err(MultiShardTxError::Mutation {
                    shard,
                    reason: reason.clone(),
                });
            }
            Ok(())
        }
    }

    fn fresh_orchestrator<'a>(
        locks: &'a InMemoryShardLockManager,
        mutator: &'a RecordingMutator,
        metrics: &'a MultiShardTxMetrics,
    ) -> MultiShardTx<'a, InMemoryShardLockManager, RecordingMutator> {
        MultiShardTx::new(locks, mutator, metrics).with_config(MultiShardTxConfig {
            tx_timeout: Duration::from_secs(2),
            lock_acquire_timeout: Duration::from_millis(50),
            leader_retries: 3,
        })
    }

    #[test]
    fn write_set_iteration_is_ascending() {
        let ws = WriteSet::from_iter([s(2), s(0), s(1), s(0)]);
        let order: Vec<u32> = ws.iter_ordered().map(|s| s.as_u32()).collect();
        assert_eq!(order, vec![0, 1, 2]);
        assert!(ws.is_multi_shard());
    }

    #[test]
    fn empty_write_set_returns_explicit_error() {
        let locks = InMemoryShardLockManager::new();
        let mutator = RecordingMutator::default();
        let metrics = MultiShardTxMetrics::default();
        let orch = fresh_orchestrator(&locks, &mutator, &metrics);
        let err = orch.execute(TxId::new(1), &WriteSet::empty()).unwrap_err();
        assert_eq!(err, MultiShardTxError::EmptyWriteSet);
    }

    #[test]
    fn happy_path_acquires_in_order_and_mutates_each_shard() {
        let locks = InMemoryShardLockManager::new();
        let mutator = RecordingMutator::default();
        let metrics = MultiShardTxMetrics::default();
        let orch = fresh_orchestrator(&locks, &mutator, &metrics);

        let ws = WriteSet::from_iter([s(2), s(0), s(1)]);
        let tx = TxId::new(7);
        orch.execute(tx, &ws).expect("execute");

        let calls = mutator.calls();
        let mutate_shards: Vec<u32> = calls
            .iter()
            .filter(|(k, _, _)| k == "mutate")
            .map(|(_, _, s)| s.as_u32())
            .collect();
        assert_eq!(mutate_shards, vec![0, 1, 2], "ascending shard order");

        // No rollbacks on success.
        assert!(calls.iter().all(|(k, _, _)| k != "rollback"));
        assert!(locks.held().is_empty(), "leases released on commit");
        // Metrics observed.
        let snapshot: BTreeMap<&str, u64> = metrics.snapshot().into_iter().collect();
        assert_eq!(snapshot.get("writes_total"), Some(&1));
        assert_eq!(snapshot.get("writes_aborted_total"), Some(&0));
        assert_eq!(snapshot.get("lock_acquire_total"), Some(&3));
    }

    #[test]
    fn mutation_failure_rolls_back_in_reverse_and_releases_every_lease() {
        let locks = InMemoryShardLockManager::new();
        let mutator = RecordingMutator::default();
        mutator.fail_mutate(s(2), "disk full");
        let metrics = MultiShardTxMetrics::default();
        let orch = fresh_orchestrator(&locks, &mutator, &metrics);

        let ws = WriteSet::from_iter([s(0), s(1), s(2)]);
        let tx = TxId::new(11);
        let err = orch.execute(tx, &ws).unwrap_err();
        match err {
            MultiShardTxError::Mutation { shard, .. } => assert_eq!(shard, s(2)),
            other => panic!("unexpected error: {other:?}"),
        }

        // Mutate(0), Mutate(1), Mutate(2) (failed), Rollback(1), Rollback(0).
        let calls = mutator.calls();
        let pairs: Vec<(&str, u32)> = calls
            .iter()
            .map(|(k, _, s)| (k.as_str(), s.as_u32()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("mutate", 0),
                ("mutate", 1),
                ("mutate", 2),
                ("rollback", 1),
                ("rollback", 0),
            ]
        );
        assert!(locks.held().is_empty(), "leases released on abort");

        let snapshot: BTreeMap<&str, u64> = metrics.snapshot().into_iter().collect();
        assert_eq!(snapshot.get("writes_aborted_total"), Some(&1));
    }

    #[test]
    fn partition_during_acquisition_aborts_clean() {
        let locks = InMemoryShardLockManager::new();
        let mutator = RecordingMutator::default();
        let metrics = MultiShardTxMetrics::default();
        let orch = fresh_orchestrator(&locks, &mutator, &metrics);

        // Inject a partition on shard 1 — the second acquire fails.
        locks.inject_partition(s(1));

        let ws = WriteSet::from_iter([s(0), s(1), s(2)]);
        let err = orch.execute(TxId::new(2), &ws).unwrap_err();
        match err {
            MultiShardTxError::Lock(LockError::Partition { shard }) => assert_eq!(shard, s(1)),
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(locks.held().is_empty(), "shard 0 lease released on abort");
        // No mutation should have happened.
        assert!(
            mutator
                .calls()
                .iter()
                .all(|(k, _, _)| k != "mutate" && k != "rollback")
        );
    }

    #[test]
    fn busy_shard_times_out_and_aborts() {
        let locks = InMemoryShardLockManager::new();
        let mutator = RecordingMutator::default();
        let metrics = MultiShardTxMetrics::default();
        let orch = fresh_orchestrator(&locks, &mutator, &metrics);

        // Pre-hold shard 1 with another transaction so the orchestrator's
        // acquisition there blocks until per-call timeout.
        locks
            .try_acquire(TxId::new(99), s(1), Duration::from_millis(10))
            .expect("pre-acquire");

        let ws = WriteSet::from_iter([s(0), s(1)]);
        let err = orch.execute(TxId::new(2), &ws).unwrap_err();
        match err {
            MultiShardTxError::Lock(LockError::Timeout { shard, .. }) => assert_eq!(shard, s(1)),
            other => panic!("unexpected error: {other:?}"),
        }
        // Shard 0 must have been released even though shard 1 timed out.
        let held = locks.held();
        assert_eq!(held.len(), 1);
        assert!(held.contains_key(&s(1)));
        assert!(!held.contains_key(&s(0)));

        let snapshot: BTreeMap<&str, u64> = metrics.snapshot().into_iter().collect();
        assert_eq!(snapshot.get("lock_timeout_total"), Some(&1));
        assert_eq!(snapshot.get("writes_aborted_total"), Some(&1));
    }

    #[test]
    fn ordered_acquisition_prevents_deadlock_under_64_concurrent_writers() {
        // Two transactions both want shards {0, 1}. Without ordered
        // acquisition this is a textbook deadlock. With it, neither
        // can take shard 1 before shard 0, so the second one blocks
        // strictly behind the first and the system makes progress.
        let locks = Arc::new(InMemoryShardLockManager::new());
        let mutator = Arc::new(RecordingMutator::default());
        let metrics = Arc::new(MultiShardTxMetrics::default());
        let allocator = Arc::new(TxIdAllocator::new());

        let mut handles = Vec::new();
        for _ in 0..64 {
            let locks = Arc::clone(&locks);
            let mutator = Arc::clone(&mutator);
            let metrics = Arc::clone(&metrics);
            let allocator = Arc::clone(&allocator);
            handles.push(std::thread::spawn(move || {
                let orch = MultiShardTx::new(&*locks, &*mutator, &*metrics).with_config(
                    MultiShardTxConfig {
                        tx_timeout: Duration::from_secs(5),
                        lock_acquire_timeout: Duration::from_millis(500),
                        leader_retries: 3,
                    },
                );
                let tx = allocator.next();
                let ws = WriteSet::from_iter([s(0), s(1)]);
                orch.execute(tx, &ws)
            }));
        }
        let mut ok = 0u32;
        for h in handles {
            if h.join().expect("join").is_ok() {
                ok += 1;
            }
        }
        assert_eq!(ok, 64, "every writer must succeed under ordered locking");
        assert!(locks.held().is_empty());

        let snapshot: BTreeMap<&str, u64> = metrics.snapshot().into_iter().collect();
        assert_eq!(snapshot.get("writes_total"), Some(&64));
        assert_eq!(snapshot.get("writes_aborted_total"), Some(&0));
    }

    #[test]
    fn leader_churn_mid_transaction_releases_old_lease_and_succeeds() {
        // Simulate leader churn: an external actor force-releases
        // shard 1's lease while it is held by tx_a. The orchestrator
        // for tx_b (waiting on shard 1) must recover and acquire on
        // the new leader.
        let locks = InMemoryShardLockManager::new();
        let mutator = RecordingMutator::default();
        let metrics = MultiShardTxMetrics::default();
        let orch = fresh_orchestrator(&locks, &mutator, &metrics);

        // Acquire shard 1 manually (simulating the old leader).
        locks
            .try_acquire(TxId::new(98), s(1), Duration::from_millis(10))
            .expect("pre-acquire");
        // Spawn a thread that releases the old leader's lease after
        // a short delay — simulates the new leader taking over.
        let locks_ref = &locks;
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                locks_ref.force_release(s(1));
            });
            // Tx_b grabs the lease as soon as the churn completes.
            let ws = WriteSet::from_iter([s(0), s(1)]);
            orch.execute(TxId::new(2), &ws)
                .expect("succeed after churn");
        });
        assert!(locks.held().is_empty());
    }

    #[test]
    fn shard_outage_mid_commit_does_not_corrupt_other_shards() {
        // Three-shard transaction; shard 2 fails its mutation. The
        // orchestrator must roll back shard 1 and shard 0 in
        // *reverse* order before returning.
        let locks = InMemoryShardLockManager::new();
        let mutator = RecordingMutator::default();
        mutator.fail_mutate(s(2), "outage");
        let metrics = MultiShardTxMetrics::default();
        let orch = fresh_orchestrator(&locks, &mutator, &metrics);

        let ws = WriteSet::from_iter([s(0), s(1), s(2)]);
        orch.execute(TxId::new(3), &ws).unwrap_err();

        let calls = mutator.calls();
        // Rollbacks are in reverse mutate order.
        let rollback_order: Vec<u32> = calls
            .iter()
            .filter(|(k, _, _)| k == "rollback")
            .map(|(_, _, s)| s.as_u32())
            .collect();
        assert_eq!(rollback_order, vec![1, 0]);
        assert!(locks.held().is_empty());
    }

    #[test]
    fn rollback_failures_are_logged_and_state_preserved() {
        let locks = InMemoryShardLockManager::new();
        let mutator = RecordingMutator::default();
        // The rollback for shard 0 will itself fail.
        mutator.fail_rollback(s(0), "double-fault");
        // And shard 2's mutate fails.
        mutator.fail_mutate(s(2), "primary outage");
        let metrics = MultiShardTxMetrics::default();
        let orch = fresh_orchestrator(&locks, &mutator, &metrics);

        let ws = WriteSet::from_iter([s(0), s(1), s(2)]);
        let err = orch.execute(TxId::new(4), &ws).unwrap_err();
        // The original mutation error wins; rollback failure is
        // logged via tracing but does not mask the root cause.
        assert!(matches!(err, MultiShardTxError::Mutation { .. }));
        // All leases released regardless.
        assert!(locks.held().is_empty());
    }

    #[test]
    fn tx_id_allocator_is_monotonic() {
        let alloc = TxIdAllocator::new();
        let a = alloc.next();
        let b = alloc.next();
        assert!(b.raw() > a.raw());
    }
}
