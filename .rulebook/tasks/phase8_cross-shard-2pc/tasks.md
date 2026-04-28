## 1. Design
- [ ] 1.1 Capture decision: pessimistic-locking-first vs 2PC-first via `rulebook_decision_create`
- [ ] 1.2 Spec the read-consistency contract across shards in `docs/specs/cluster-transactions.md`
- [ ] 1.3 Spec the deadlock-prevention rule (ordered acquisition by `shard_id`)

## 2. Lock acquisition
- [ ] 2.1 Implement per-shard write-lock primitive (Raft-replicated lease) in `sharding/raft/`
- [ ] 2.2 Coordinator: build write-set per transaction (which shards will be written)
- [ ] 2.3 Coordinator: acquire locks in `shard_id` ascending order
- [ ] 2.4 Coordinator: implement `tx_timeout_ms` with clean abort

## 3. Multi-shard mutation execution
- [ ] 3.1 Coordinator: execute per-shard mutations against acquired leases
- [ ] 3.2 Coordinator: commit all shards atomically (all-or-nothing on success path)
- [ ] 3.3 Coordinator: rollback all shards on any per-shard error
- [ ] 3.4 Surface `nexus_cluster_multi_shard_writes_total` + `_aborted_total` metrics

## 4. Failure handling
- [ ] 4.1 Handle leader churn mid-write: re-acquire lease on new leader (3-attempt retry)
- [ ] 4.2 Handle network partition: abort with clear `ERR_PARTITION` if quorum lost
- [ ] 4.3 Handle slow-disk: per-shard timeout independent of global tx timeout

## 5. Chaos tests
- [ ] 5.1 Test: leader churn during multi-shard write — assert atomic outcome
- [ ] 5.2 Test: network partition during multi-shard write — assert clean abort
- [ ] 5.3 Test: 64 concurrent multi-shard writes — assert no deadlock
- [ ] 5.4 Test: shard outage mid-commit — assert all-or-nothing

## 6. Documentation
- [ ] 6.1 Update `docs/CLUSTER_MODE.md` with multi-shard-write contract
- [ ] 6.2 Document `tx_timeout_ms` tuning guidance
- [ ] 6.3 CHANGELOG entry under V2 hardening
- [ ] 6.4 Spec follow-up: phase 9 full 2PC with prepare/commit log entries

## 7. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 7.1 Update or create documentation covering the implementation
- [ ] 7.2 Write tests covering the new behavior
- [ ] 7.3 Run tests and confirm they pass
