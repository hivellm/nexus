# Tasks: phase0_perf-store-reverse-incoming-adjacency-index

The store has no authoritative reverse (incoming) adjacency, so incoming-edge
liveness is either a full scan or the non-authoritative `RelationshipIndex`
hint. This keeps `node_has_live_relationship` (incoming tier) and
`delete_node_relationships` at O(total edges). See proposal.md.

## 1. Implementation
- [x] 1.1 Confirm the single write chokepoint — CONFIRMED FOR CREATION, REFUTED
  FOR DELETION. Creation does funnel through `create_relationship` (engine CRUD,
  executor CREATE ×3, bulk loader). Deletion has FIVE paths and only two call
  `delete_rel`: Cypher `DELETE r`, `DETACH DELETE`, and the graph API read the
  record, `mark_deleted()`, and `write_rel` it back. The real universal funnel
  is one level lower — `RecordStore::write_rel`, the only writer of `rels_mmap`
  — and nothing anywhere reassigns `src_id`/`dst_id` on an existing record.
  Written up in `docs/analysis/store-adjacency-index/01_write_chokepoint.md`.
- [x] 1.2 Decided: in-memory, both directions, owned by the store
  (`storage::adjacency_index::AdjacencyIndex`, `Arc`-shared across clones).
  On-disk incoming chains rejected — a record-format change plus migration,
  buying only durability-without-rebuild, and the rebuild is free. Recorded in
  `docs/analysis/store-adjacency-index/02_structure_decision.md`, which also
  argues why BOTH directions are needed rather than incoming alone: 1.5 asks to
  drop the full scan, and that is only safe if the outgoing half is
  authoritative too (the `first_rel_ptr` walk is explicitly best-effort).
- [x] 1.3 Maintained in `write_rel` — one add-or-remove decided by the record's
  own deleted bit, no read-before-write — instead of in `create_relationship` +
  N deletion paths. This is what makes it authoritative for write paths that do
  not know it exists. `clear_all` (which re-maps the files wholesale, bypassing
  `write_rel`) clears it explicitly.
- [x] 1.4 Rebuilt inside the scan `RecordStore::new` ALREADY performs to derive
  `next_rel_id` — zero extra I/O, and it covers every store user rather than
  only `Engine::new` (the task suggested `engine/mod.rs`; the store is the
  stronger locus, see the analysis). Deleted records are skipped, so a reopen
  cannot resurrect a deleted edge.
- [x] 1.5 `node_has_live_relationship` is now a single O(degree) pass over
  `connected_relationships`, re-reading each candidate record. The best-effort
  `first_rel_ptr` walk AND the O(total relationships) scan are both gone.
- [x] 1.6 `delete_node_relationships` (DETACH DELETE) likewise iterates
  `connected_relationships` instead of `0..relationship_count()`.
- [x] 1.7 MERGE exact-edge: the `cache::RelationshipIndex` hint is unchanged
  (still populated by the engine path only), but its FALLBACK — previously a
  `first_rel_ptr`/`next_src_ptr` chain walk that depended on chain integrity,
  where one broken link ended the walk early and made MERGE re-create an
  existing edge — is now the store's authoritative outgoing adjacency. Same
  O(out-degree), no chain-integrity dependency, hub telemetry preserved.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  — `docs/specs/storage-format.md` gains an "Adjacency index" section (the
  maintenance invariant, the rebuild, why it is an accelerator and not an
  oracle), plus the two analysis files above.
- [x] 2.2 Write tests covering the new behavior
  — 11 integration tests in `tests/storage/reverse_adjacency_index_test.rs`
  (every create path: Cypher CREATE, engine CRUD, raw `write_rel`; clone
  sharing; rebuild on reopen; deleted edges not resurrected by the rebuild;
  incoming-only delete guard; DETACH clearing a fan-in and a self-loop;
  create/delete churn staying exact), 6 unit tests in
  `storage::adjacency_index`, and one bulk-loader test in `loader::tests`.
- [x] 2.3 Run tests and confirm they pass
  — `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green, 0 failed.

## Out of scope (still full scans, by design)
`rebuild_relationship_index_from_storage`, compaction/maintenance, clustering,
and constraint validation still walk `0..relationship_count()`. Each is either
inherently a whole-store pass or off the hot path; the two paths this task
targets are O(degree) end to end.

## Related
- `phase0_perf-delete-node-relationship-check-full-scan` — did the outgoing half
  (O(out-degree) fast path); this task does the incoming half authoritatively.
- `phase0_fix-cypher-relationship-delete-noop` — deletion path this index must
  stay consistent with.
