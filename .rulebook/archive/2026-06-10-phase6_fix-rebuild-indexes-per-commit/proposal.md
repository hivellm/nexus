# Proposal: phase6_fix-rebuild-indexes-per-commit

Source: GitHub issue #15 (https://github.com/hivellm/nexus/issues/15)

## Why
Every explicit-transaction `COMMIT` calls `rebuild_indexes_from_storage()`
(`crates/nexus-core/src/engine/mod.rs:3831`), a full O(N_nodes + N_rels)
scan that repopulates the label + relationship indexes — even for a
single-row transaction. `apply_pending_index_updates` (mod.rs:3817)
already maintains the indexes incrementally right above it. Since the
2.3.2 index-durability fix (#11), `rebuild_indexes_from_storage` also
re-creates + re-backfills every property index, so each explicit COMMIT
is now O(N x indexes) — a scaling cliff that serializes
transaction-heavy workloads.

## What Changes
- Remove the `rebuild_indexes_from_storage()` call at mod.rs:3831; rely on
  the incremental maintenance (`label_index.add_node` crud.rs:605,
  `relationship_index().add_relationship` crud.rs:994,
  `apply_pending_index_updates` mod.rs:3817) that already keeps indexes
  current at commit.
- Verify rollback still leaves indexes consistent (the incremental updates
  are applied from the session's pending set, not from uncommitted state).
- Add a dev/test assertion that a post-commit rebuild produces no net diff
  vs the incremental result.

## Impact
- Affected specs: transaction / commit, indexing
- Affected code: `crates/nexus-core/src/engine/mod.rs` (explicit-COMMIT path)
- Breaking change: NO
- User benefit: explicit-transaction COMMIT latency no longer scales with
  graph size; removes the #11-amplified per-commit property-index backfill.

## Notes
- Audit finding #1 (companion to #11/#12). Pairs with #16 (refresh_executor
  cost) for the largest sustained-ingest throughput win.
