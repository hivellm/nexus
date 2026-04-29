# Cluster transactions — multi-shard writes

> Phase 8 contract. Spec for the multi-shard write protocol added by
> [`phase8_cross-shard-2pc`](../../.rulebook/archive/) and implemented
> in [`crates/nexus-core/src/coordinator/multi_shard_tx.rs`](../../crates/nexus-core/src/coordinator/multi_shard_tx.rs).
> See [ADR-016](../../.rulebook/decisions/) for the
> pessimistic-locking-vs-2PC decision.

## Scope

This spec defines the contract for transactions whose write set
spans more than one shard. Single-shard writes stay on the
existing fast path (per-shard Raft, no coordinator-side locking)
and are out of scope here.

A transaction is **multi-shard** when
[`WriteSet::is_multi_shard()`](../../crates/nexus-core/src/coordinator/multi_shard_tx.rs)
returns `true` — i.e. the planner produced subplans for two or
more distinct `shard_id`s. The coordinator builds the
[`WriteSet`](../../crates/nexus-core/src/coordinator/multi_shard_tx.rs)
before any locks are taken; the set is immutable for the duration
of the transaction.

## Atomicity contract

A multi-shard write is atomic in both directions:

* **All-or-nothing on commit**: every shard in the write set sees
  the mutation, or none do. Partial commits are not externally
  observable.
* **All-or-nothing on abort**: any shard error during the mutation
  phase rolls back every previously-mutated shard via
  [`ShardMutator::rollback`](../../crates/nexus-core/src/coordinator/multi_shard_tx.rs).
  The rollback is idempotent so the orchestrator can safely call
  it on a shard whose `mutate` errored mid-write.

The orchestrator NEVER returns half-results to the client.
Coordinator-internal failures (release after successful mutate,
non-recoverable rollback errors) are logged at WARN / ERROR but do
not change the visible transaction outcome.

## Deadlock prevention

The coordinator acquires write leases in **strict ascending
`shard_id` order**, then mutates in the same order, then releases
in the **reverse** order. Two transactions whose write sets
overlap cannot form a wait-for cycle: with a total order over
shards, Coffman's circular-wait condition fails. This is the
classical
[Havender resource-ordering](https://en.wikipedia.org/wiki/Deadlock_prevention_algorithms)
result; the implementation relies on it without further proof.

Concretely: tx_A wants shards `{0, 1}`, tx_B wants `{0, 1}`. With
ordered acquisition, tx_B blocks behind tx_A on shard 0; once
tx_A releases shard 0, tx_B acquires it, then 1, then commits.
No deadlock is possible. The
[`ordered_acquisition_prevents_deadlock_under_64_concurrent_writers`](../../crates/nexus-core/src/coordinator/multi_shard_tx.rs)
unit test pins this for 64 concurrent writers.

## Read consistency

* **Inside a transaction**: every read sees a snapshot pinned at
  acquisition time on every shard. Snapshot isolation, same
  semantics single-shard MVCC gives.
* **Outside a transaction** (cross-shard read without an enclosing
  multi-shard transaction): eventually consistent across shards.
  The same contract Nexus single-node MVCC gives readers vs.
  concurrent writers, lifted to the shard boundary.

This means a read-your-writes guarantee holds inside a
transaction and across single-shard writes, but does NOT hold
across two independent multi-shard writes that happen to overlap
in real time. Callers needing strict cross-write ordering must
wrap the writes in a single `BEGIN ... COMMIT` block.

## Lease lifecycle

A lease on shard `S` for transaction `T` has three states:

```text
   try_acquire()        mutate()              release()
unheld ──────────► held(T) ──────────► held(T) ──────────► unheld
        per-call             tx-wide            in reverse order
        timeout              timeout
```

Every state has a hard deadline:

* **Acquire**: the lease holder must release within
  `lock_acquire_timeout` (default 500 ms). Beyond that, the
  acquirer surfaces `ERR_LOCK_TIMEOUT` and the orchestrator
  aborts the transaction.
* **Mutate**: the per-shard mutation must complete within the
  remaining budget of `tx_timeout` (default 5 s). A slow shard
  surfaces `ERR_TX_TIMEOUT` and the orchestrator aborts.
* **Release**: best effort. A release failure after a successful
  mutate is a bookkeeping problem (the mutation already
  committed inside the per-shard Raft group); WARN log only.

If the coordinator crashes mid-transaction, every acquired lease
times out on the shard side without manual intervention — that's
the headline benefit of pessimistic locking over 2PC.

## Failure modes

| Code | Cause | Recovery |
|---|---|---|
| `ERR_LOCK_BUSY` | A different transaction holds the lease. | The orchestrator retries until `lock_acquire_timeout` elapses, then surfaces `ERR_LOCK_TIMEOUT`. |
| `ERR_LOCK_TIMEOUT` | Acquisition timed out. | Caller retries the whole transaction. |
| `ERR_PARTITION` | The shard's Raft group lost quorum during acquisition. | Caller retries against the recovered cluster; the orchestrator releases any leases already held. |
| `ERR_SHARD_FAILURE` | Non-recoverable shard error (disk full, OOM). | The orchestrator aborts; operator must reconcile. |
| `ERR_SHARD_MUTATION` | The per-shard mutation rejected the work (constraint violation, type error). | Same as a single-shard mutation rejection — surface to the client; the orchestrator rolls back the shards already mutated. |
| `ERR_ROLLBACK_FAILED` | Rollback itself failed for ≥ 1 shard. | The cluster is in a manually-recoverable state; the operator inspects logs. The original mutation error remains the visible cause. |
| `ERR_TX_TIMEOUT` | Outer `tx_timeout` exhausted. | Caller retries the whole transaction. |
| `ERR_EMPTY_WRITE_SET` | Caller passed an empty `WriteSet` — a coordinator-internal bug. | Surfaced to logs; never returned to clients. |

## Tunable parameters

Defaults documented as `MultiShardTxConfig::default()`.

| Field | Default | Notes |
|---|---|---|
| `tx_timeout` | 5 s | Outer wall-clock deadline. Aborts the transaction if exceeded at any phase. |
| `lock_acquire_timeout` | 500 ms | Per-call lock acquisition timeout. Smaller than `tx_timeout` so a slow shard cannot consume the whole budget. |
| `leader_retries` | 3 | Number of leader-churn retries per shard during acquisition. On `ERR_NOT_LEADER` we re-resolve the leader and try once more, up to this cap. |

Tuning guidance: increase `tx_timeout` only if your application
genuinely needs to perform very large multi-shard mutations
(thousands of nodes per shard). Decrease `lock_acquire_timeout`
if your workload has many short multi-shard transactions and
you want to fail fast under contention rather than wait. Do not
tune `leader_retries` upward — leader churn that exceeds 3
re-elections in 5 s usually indicates a sicker problem than this
budget can paper over.

## Observability

Four counters surface to Prometheus via the standard `/metrics`
endpoint:

```text
nexus_cluster_multi_shard_writes_total          counter
nexus_cluster_multi_shard_writes_aborted_total  counter
nexus_cluster_multi_shard_lock_acquire_total    counter
nexus_cluster_multi_shard_lock_timeout_total    counter
```

Recommended dashboards:

* **Abort ratio**: `aborted_total / writes_total`. A sustained
  > 5 % is a red flag: either contention is too high and
  `lock_acquire_timeout` should be increased, or a shard is
  consistently failing.
* **Lease wait time**: distribution of acquisition durations
  (sampled via tracing histograms; not Prometheus-counter-able).
* **Cross-tx fairness**: per-shard lock-acquire counter — a
  hot shard skewing the distribution suggests a re-shard is
  due.

## Forward compatibility

The public API
([`MultiShardTx::execute(tx, write_set)`](../../crates/nexus-core/src/coordinator/multi_shard_tx.rs))
hides which protocol is in use. A future migration to full
cross-shard 2PC (with prepare / commit log entries on every
shard's Raft group) keeps the same external surface; only the
internals of `execute` change. That work is tracked under
`phase9_full-2pc-cross-shard`.

## See also

- [ADR — pessimistic-locking-vs-2PC](../../.rulebook/decisions/) — the decision rationale.
- [`docs/CLUSTER_MODE.md`](../CLUSTER_MODE.md) — operator runbook.
- [`crates/nexus-core/src/coordinator/multi_shard_tx.rs`](../../crates/nexus-core/src/coordinator/multi_shard_tx.rs) — implementation.
- [Havender resource ordering — Wikipedia](https://en.wikipedia.org/wiki/Deadlock_prevention_algorithms)
