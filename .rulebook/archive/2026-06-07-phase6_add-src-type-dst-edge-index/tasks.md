## 1. Implementation
- [x] 1.1 Added an exact-edge existence index `(src_id, type_id, dst_id) -> Vec<rel_id>` to `cache::relationship_index::RelationshipIndex`, maintained in `add_relationship` / `remove_relationship` / `clear`, with a verified-hint `find_edge` accessor
- [x] 1.2 `find_relationship_between` (engine) consults `find_edge` first (O(1)), verifies the candidate against storage (not deleted, src/dst/type match), and falls back to the source-chain walk on any miss — correctness never depends on the index being complete
- [x] 1.3 Rebuilt the relationship index (type/node/edge) from storage at startup in `rebuild_indexes_from_storage` so the fast path survives a restart (the re-bootstrap write-burst scenario); added `Engine::flush()` for durable reopen

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation (CHANGELOG [Unreleased])
- [x] 2.2 Write tests covering the new behavior (`tests/edge_merge_index_test.rs`: edge-MERGE idempotent in-session and after reopen via the rebuilt index)
- [x] 2.3 Run tests and confirm they pass (nexus-core 2354 lib + integration green; clippy/fmt clean)
