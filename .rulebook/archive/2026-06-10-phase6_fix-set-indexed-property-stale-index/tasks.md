## 1. Implementation
- [x] 1.1 Refresh the typed property index on SET/REMOVE/label changes — `persist_node_state` captures the pre-write bag + labels and calls the new `typed_index_refresh_node` (evict old `(label, key, value)` entries, re-add new, registered indexes only), joining the FTS/spatial refresh siblings
- [x] 1.2 Index non-transactional Cypher CREATE immediately — the standalone-CREATE dispatch branch captures a node-count watermark around `executor.execute` and runs the new `index_typed_properties_for_new_nodes` over the allocated id range (found while writing the SET test: the executor CREATE never populated the typed index, so the SET's own MATCH-seek no-opped)

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [Unreleased] Fixed entries (both bugs)
- [x] 2.2 Write tests covering the new behavior — `set_on_indexed_property_updates_typed_index` (TDD red→green: found by new value via seek with no scan notification, not found by old value, REMOVE evicts) and `cypher_create_maintains_typed_index_immediately` (find_exact hit + seek without scan right after CREATE)
- [x] 2.3 Run tests and confirm they pass — both new tests green; full `cargo test -p nexus-core --lib` 2386/2386 (three consecutive runs); clippy 0 warnings; fmt applied
