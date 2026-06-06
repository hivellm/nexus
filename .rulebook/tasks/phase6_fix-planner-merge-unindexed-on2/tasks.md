## 1. Investigation
- [x] 1.1 Mapped MERGE planning + executor: MERGE bypasses the operator pipeline and runs through engine `execute_write_query` -> `process_merge_clause` / `process_merge_relationship`
- [x] 1.2 Node-MERGE existence lookup (`find_nodes_by_node_pattern`, crud.rs) did a label-bitmap scan + per-candidate property compare — O(N_label), never used the property B-tree
- [x] 1.3 Edge-MERGE existence (`find_relationship_between`, mod.rs) walks the SOURCE node's relationship chain — O(out-degree of src), source-scoped (NOT a global rel scan)
- [x] 1.4 Confirmed the dominant quadratic driver was node resolution: a batch of M merges over N label nodes is O(M*N); edge-MERGE resolves its endpoints through the same node lookup

## 2. Implementation — node MERGE
- [x] 2.1 `find_nodes_by_node_pattern` seeks via `property_index.find_exact` (intersecting per-property bitmaps) when a covering index exists; verifies candidates; falls back to the label scan otherwise
- [x] 2.2 Fallback + unindexed warning retained when no index exists
- [x] 2.3 Typed property index kept in sync on create (`maintain_indexed_properties`) so post-CREATE-INDEX nodes are seekable — fixes a MERGE-idempotency duplicate the seek would otherwise cause

## 3. Implementation — edge MERGE
- [x] 3.1 Endpoint resolution for edge-MERGE goes through the now-index-backed node lookup (the quadratic cost). `find_relationship_between` is O(out-degree of source), correct, and not the global-scan culprit.
- [x] 3.2 MERGE semantics verified (create-if-absent, match-if-present, idempotent) with and without an index
- [ ] 3.3 FOLLOW-UP (separate task): a `(src, type, dst)` relationship hash index for true O(1) edge existence — only needed for a single hub accumulating very high same-type degree; current O(out-degree) is acceptable for typical workloads

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 4.1 Update or create documentation covering the implementation (CHANGELOG [2.3.0])
- [x] 4.2 Write tests covering the new behavior (`tests/merge_index_correctness_test.rs` — idempotency with/without index; `tests/unindexed_property_notification_e2e_test.rs` made deterministic)
- [x] 4.3 Run tests and confirm they pass (nexus-core 2354 lib + integration green; clippy/fmt clean)

## 5. Release unblock
- [x] 5.1 Fixed the flaky `unindexed_property_notification_e2e_test`: root cause was the shared test catalog bleeding index/label state across parallel tests + the notification not firing for un-interned keys + stale thread-local notifications on the write path. Now uses isolated catalogs; notification fires for un-interned keys; write path drains stale notifications. 8/8 deterministic.
