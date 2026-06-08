## 1. Investigation
- [x] 1.1 Reproduce: `UNWIND [...] AS row MERGE (n:L {id:row.id}) SET n.name=row.nm RETURN count(n)` returns 200 / count 0 and persists nothing
- [x] 1.2 Map the write-path dispatch for UNWIND + write clauses (where `execute_write_query` / dispatch handles UNWIND) and identify why the row list does not drive the per-row write

## 2. Implementation
- [x] 2.1 Iterate UNWIND rows for write clauses (MERGE/CREATE/SET/REMOVE/FOREACH), binding the row variable per iteration against a fresh per-row context so every row persists in one statement (relationship CREATE inside UNWIND errors clearly rather than silently dropping)
- [x] 2.2 Ensure `RETURN`/aggregates (e.g. `count(n)`) reflect the rows actually written; route UNWIND+write through the engine write path on both the engine dispatch and the REST server

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the implementation (CHANGELOG Fixed / GH #13)
- [x] 3.2 Write tests: UNWIND-driven MERGE/CREATE/SET persists every row (count matches, data readable, per-row SET correctness); idempotent MERGE
- [x] 3.3 Run tests and confirm they pass
