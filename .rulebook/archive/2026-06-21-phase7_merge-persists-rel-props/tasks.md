## 1. Implementation
- [x] 1.1 Fix A: in `process_merge_relationship`, evaluate `rel_pattern.properties` and pass them to `create_relationship` on the create branch (ON CREATE SET still layered on top); MERGE now persists inline rel props — done (write_exec.rs; uses `eval_write_value` so UNWIND `row.*` resolve)
- [x] 1.2 Fix B (bind): extend `process_match_clause_multi` to bind a matched `(a)-[r:T]->(b)` relationship variable into a rel context (find rels between the bound node pair by type, honouring direction) — done; threaded the new `rel_context` param through all 3 callers
- [x] 1.3 Fix B (apply): thread the rel context into `apply_set_clause`; resolve rel-variable targets and update relationship properties for `SET r.k = v` and `SET r += {…}` via `update_relationship_properties` — done; added `set_relationship_property` + `merge_relationship_map` helpers (null value removes the key); threaded `rel_context` through all 5 `apply_set_clause` callers

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [2.3.4] entry; version bumped to 2.3.4 across Cargo.toml / Dockerfile / README badge (consistency check OK)
- [x] 2.2 Write tests covering the new behavior — 4 tests in `engine/tests/write.rs`: `merge_persists_inline_relationship_properties`, `merge_relationship_idempotent_keeps_props`, `set_on_matched_relationship_variable_persists`, `set_map_merge_on_relationship_variable` (string+int props; idempotency; SET r.k and SET r += with null-removes-key)
- [x] 2.3 Run tests and confirm they pass — 4/4 new tests green; existing edge tests (`unwind_match_merge` 2/2) green; full `cargo test -p nexus-core --lib` 2391/2391; clippy 0 warnings; fmt applied
