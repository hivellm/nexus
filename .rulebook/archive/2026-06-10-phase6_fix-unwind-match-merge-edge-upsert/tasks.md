## 1. Investigation
- [x] 1.1 Reproduce: `UNWIND [...] AS row MATCH (a {id:row.fk}),(b {id:row.tk}) MERGE (a)-[r:T]->(b) ON CREATE/ON MATCH SET r.w=row.w RETURN count(r)` errors "Unsupported clause after UNWIND in write query" — reproduced and fixed in 6093bfcb
- [x] 1.2 Confirm the post-UNWIND loop in `execute_unwind_write_query` rejects MATCH; verify `find_nodes_by_node_pattern` resolves `{id: row.fk}` against the unwind binding and that `process_merge_relationship` + ON CREATE/ON MATCH SET can apply per row — confirmed in 6093bfcb (per-row `process_match_clause_multi`; `apply_merge_rel_set` helper)

## 2. Implementation
- [x] 2.1 Allow per-row MATCH after UNWIND: run MATCH against a fresh per-row context (binding `row.fk`/`row.tk`), then resolve the relationship MERGE endpoints from it — shipped in 6093bfcb (Clause::Match arm in the post-UNWIND loop)
- [x] 2.2 Apply relationship MERGE with ON CREATE / ON MATCH SET per row so the edge upserts for every row; `RETURN count(r)` reflects all rows — ON CREATE/ON MATCH SET shipped in 6093bfcb; the `count(r)` half is completed here: rel bindings now accumulate per row (`HashMap<String, Vec<(u64, String)>>` — the previous per-variable insert kept only the last edge) and `evaluate_return_expression_with_rels` implements `count(r)` over the deduped edge ids (was falling through to `null`). Distinct-edge semantics match the #13 node-count behavior.

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the implementation (CHANGELOG Fixed / GH #14) — CHANGELOG [Unreleased] Fixed entry
- [x] 3.2 Write tests: UNWIND+MATCH+edge-MERGE upserts every row (count matches, edges + properties readable, idempotent across rows) — `unwind_match_merge_edge_upsert` (single row: create, ON CREATE SET, idempotent, ON MATCH SET) + new `unwind_match_merge_edge_upsert_every_row` (2 rows: count(r)=2, per-row property values readable)
- [x] 3.3 Run tests and confirm they pass — both tests green; full `cargo test -p nexus-core --lib` 2379/2379; clippy 0 warnings; fmt applied
