## 1. Investigation
- [ ] 1.1 Reproduce: `UNWIND [...] AS row MERGE (n:L {id:row.id}) SET n.name=row.nm RETURN count(n)` returns 200 / count 0 and persists nothing
- [ ] 1.2 Map the write-path dispatch for UNWIND + write clauses (where `execute_write_query` / dispatch handles UNWIND) and identify why the row list does not drive the per-row write

## 2. Implementation
- [ ] 2.1 Iterate UNWIND rows for write clauses (MERGE/CREATE/SET, and row-driven DELETE/REMOVE), binding the row variable per iteration so every row persists in one statement
- [ ] 2.2 Ensure `RETURN`/aggregates (e.g. `count(n)`) reflect the rows actually written; run the batch in a single transaction

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the implementation (CHANGELOG Fixed / GH #13)
- [ ] 3.2 Write tests: UNWIND-driven MERGE/CREATE/SET persists every row (count matches, data readable); standalone-vs-UNWIND parity
- [ ] 3.3 Run tests and confirm they pass
