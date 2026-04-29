## 1. Design
- [x] 1.1 Capture decision: pessimistic-locking-first vs 2PC-first via `rulebook_decision_create` — ADR-009 (`cross-shard-write-atomicity-pessimistic-ordered-locking-before-full-2pc`)
- [x] 1.2 Spec the read-consistency contract across shards in `docs/specs/cluster-transactions.md`
- [x] 1.3 Spec the deadlock-prevention rule (ordered acquisition by `shard_id`)

## 2. Lock acquisition
- [x] 2.1 Implement per-shard write-lock primitive — `ShardLockManager` trait + `InMemoryShardLockManager` (Raft-replicated wiring is the production impl swap; the trait is the seam)
- [x] 2.2 Coordinator: build write-set per transaction — `WriteSet` (BTreeSet over shards, deduped, ordered)
- [x] 2.3 Coordinator: acquire locks in `shard_id` ascending order — `MultiShardTx::execute` walks `write_set.iter_ordered()`
- [x] 2.4 Coordinator: implement `tx_timeout_ms` with clean abort — `MultiShardTxConfig::tx_timeout` + `ERR_TX_TIMEOUT` + unwind on every exit path

## 3. Multi-shard mutation execution
- [x] 3.1 Coordinator: execute per-shard mutations against acquired leases — `ShardMutator::mutate` invoked once per shard in ascending order
- [x] 3.2 Coordinator: commit all shards atomically — release-leases-in-reverse on the success path
- [x] 3.3 Coordinator: rollback all shards on any per-shard error — `ShardMutator::rollback` called in reverse mutate order before any release on the abort path
- [x] 3.4 Surface metrics — `MultiShardTxMetrics` with the four documented counters

## 4. Failure handling
- [x] 4.1 Handle leader churn mid-write — `leader_retries` config + the `leader_churn_mid_transaction_releases_old_lease_and_succeeds` test pins the recovery
- [x] 4.2 Handle network partition — `LockError::Partition` surfaces `ERR_PARTITION`; `partition_during_acquisition_aborts_clean` test pins the abort-with-cleanup path
- [x] 4.3 Handle slow-disk — `lock_acquire_timeout` is independent of `tx_timeout` so a slow shard cannot consume the whole budget; `busy_shard_times_out_and_aborts` pins this

## 5. Chaos tests
- [x] 5.1 Leader churn during multi-shard write — `leader_churn_mid_transaction_releases_old_lease_and_succeeds`
- [x] 5.2 Network partition during multi-shard write — `partition_during_acquisition_aborts_clean`
- [x] 5.3 64 concurrent multi-shard writes — `ordered_acquisition_prevents_deadlock_under_64_concurrent_writers`
- [x] 5.4 Shard outage mid-commit — `shard_outage_mid_commit_does_not_corrupt_other_shards` + `rollback_failures_are_logged_and_state_preserved`

## 6. Documentation
- [x] 6.1 Update cluster docs — `docs/guides/DISTRIBUTED_DEPLOYMENT.md` got the new "Multi-shard write path" section. `docs/CLUSTER_MODE.md` is about the *multi-tenant* cluster mode (catalog-prefix isolation), distinct from V2 sharding, so no edit there
- [x] 6.2 Document `tx_timeout_ms` tuning guidance — in `docs/specs/cluster-transactions.md` § "Tunable parameters"
- [x] 6.3 CHANGELOG entry — under "Added — `phase8_cross-shard-2pc`"
- [x] 6.4 Spec follow-up — `phase9_full-2pc-cross-shard` referenced in the spec + ADR + DISTRIBUTED_DEPLOYMENT "Out of scope" section

## 7. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 7.1 Update or create documentation covering the implementation
- [x] 7.2 Write tests covering the new behavior — 11 unit tests
- [x] 7.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --lib coordinator::multi_shard_tx` 11/11 green; `cargo clippy -p nexus-core --all-targets -- -D warnings` clean
