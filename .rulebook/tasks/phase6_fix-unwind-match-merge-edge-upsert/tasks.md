## 1. Investigation
- [ ] 1.1 Reproduce: `UNWIND [...] AS row MATCH (a {id:row.fk}),(b {id:row.tk}) MERGE (a)-[r:T]->(b) ON CREATE/ON MATCH SET r.w=row.w RETURN count(r)` errors "Unsupported clause after UNWIND in write query"
- [ ] 1.2 Confirm the post-UNWIND loop in `execute_unwind_write_query` rejects MATCH; verify `find_nodes_by_node_pattern` resolves `{id: row.fk}` against the unwind binding and that `process_merge_relationship` + ON CREATE/ON MATCH SET can apply per row

## 2. Implementation
- [ ] 2.1 Allow per-row MATCH after UNWIND: run MATCH against a fresh per-row context (binding `row.fk`/`row.tk`), then resolve the relationship MERGE endpoints from it
- [ ] 2.2 Apply relationship MERGE with ON CREATE / ON MATCH SET per row so the edge upserts for every row; `RETURN count(r)` reflects all rows

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the implementation (CHANGELOG Fixed / GH #14)
- [ ] 3.2 Write tests: UNWIND+MATCH+edge-MERGE upserts every row (count matches, edges + properties readable, idempotent across rows)
- [ ] 3.3 Run tests and confirm they pass
