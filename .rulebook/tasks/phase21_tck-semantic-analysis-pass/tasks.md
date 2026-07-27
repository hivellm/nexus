## 1. Implementation
- [ ] 1.1 variable scoping validation stage — 3 of 4 checks DONE, all in `executor/semantic_validation.rs`, hook in `engine/query_pipeline.rs::execute_cypher_with_context` (after parse, before plan). Each landed with the full nexus-core suite green.
  - [x] `UndefinedVariable` (commit 0ac36d07) — over-collects binders (pattern node/rel/path/QPP vars, WITH/RETURN aliases, UNWIND/FOREACH vars, list/pattern-comprehension + any/all/none/single predicate vars), flags a reference bound nowhere; bails on UNION/CALL{}/CALL-proc/LOAD CSV/DDL; excludes the `__DISTINCT__` marker; bare RETURN/WITH items are references not binders.
  - [x] `VariableTypeConflict` (commit 43bd0325) — name bound as node AND as rel across top-level MATCH/CREATE/MERGE patterns → conflict (always genuine, cannot false-positive).
  - [x] `VariableAlreadyBound` (commit 38b0e806) — CREATE re-declaring a bound var WITH labels/props (incl. intra-CREATE order); bails on WITH (needs exact monotonic scope). DEFERRED sub-cases: bare standalone `CREATE (a)` (Create1[13]), MATCH/MERGE re-binding (Match6, Merge1/Merge5), and WITH-scope-narrowing so the check runs on WITH-containing queries.
  - NOT IMPLEMENTABLE: `NoVariablesInScope` (`RETURN *`/`WITH *` empty scope) — the PARSER rejects `RETURN *`/`WITH *` outright (generic syntax error), so it never reaches the semantic pass. Depends on `RETURN *` parser support (a separate gap), then emitting the token when scope is empty.
- [ ] 1.2 aggregation placement validation
- [ ] 1.3 SKIP/LIMIT arg validation
- [ ] 1.4 projection/UNION/write structural validation
- [ ] 1.5 OpenCypherErrorKind + detail tokens emitted

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
