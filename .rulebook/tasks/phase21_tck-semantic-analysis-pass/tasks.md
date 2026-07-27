## 1. Implementation
- [ ] 1.1 variable scoping validation stage — 3 of 4 checks DONE, all in `executor/semantic_validation.rs`, hook in `engine/query_pipeline.rs::execute_cypher_with_context` (after parse, before plan). Each landed with the full nexus-core suite green.
  - [x] `UndefinedVariable` (commit 0ac36d07) — over-collects binders (pattern node/rel/path/QPP vars, WITH/RETURN aliases, UNWIND/FOREACH vars, list/pattern-comprehension + any/all/none/single predicate vars), flags a reference bound nowhere; bails on UNION/CALL{}/CALL-proc/LOAD CSV/DDL; excludes the `__DISTINCT__` marker; bare RETURN/WITH items are references not binders.
  - [x] `VariableTypeConflict` (commit 43bd0325) — name bound as node AND as rel across top-level MATCH/CREATE/MERGE patterns → conflict (always genuine, cannot false-positive).
  - [x] `VariableAlreadyBound` (commit 38b0e806) — CREATE re-declaring a bound var WITH labels/props (incl. intra-CREATE order); bails on WITH (needs exact monotonic scope). DEFERRED sub-cases: bare standalone `CREATE (a)` (Create1[13]), MATCH/MERGE re-binding (Match6, Merge1/Merge5), and WITH-scope-narrowing so the check runs on WITH-containing queries.
  - NOT IMPLEMENTABLE: `NoVariablesInScope` (`RETURN *`/`WITH *` empty scope) — the PARSER rejects `RETURN *`/`WITH *` outright (generic syntax error), so it never reaches the semantic pass. Depends on `RETURN *` parser support (a separate gap), then emitting the token when scope is empty.
- [ ] 1.2 aggregation placement validation — 2 of 3 DONE (each full-suite-gated).
  - [x] `NestedAggregation` (commit 4d11d060) — aggregate nested in an aggregate (`count(count(*))`).
  - [x] `InvalidAggregation` (commit 4d11d060) — aggregate inside a WHERE (`WHERE count(a) > 1`).
  - DEFERRED: `AmbiguousAggregationExpression` (implicit-grouping semantics — higher false-positive risk; needs grouping-key analysis).
- [x] 1.3 SKIP/LIMIT arg validation (commit 7500cea2) — `NegativeIntegerArgument` (negative int literal, incl. sign-folded) + `NonConstantExpression` (variable-dependent arg). Parameters and non-literal constants left alone. Runtime negative-parameter case (`SKIP $n` where $n<0) not statically detectable — deferred to a runtime check.
- [ ] 1.4 projection/UNION/write structural validation — NOT STARTED. Safe/contained next: `ColumnNameConflict` (duplicate RETURN/WITH output names), `InvalidDelete` (DELETE of a non-entity). Needs UNION handling (currently bailed): `DifferentColumnsInUnion`, `InvalidClauseComposition`.
- [ ] 1.5 OpenCypherErrorKind + detail tokens emitted — LARGELY SATISFIED: every check emits `Error::CypherSyntax("<Token>: …")` which classifies as `SyntaxError` and carries the CamelCase token the TCK runner matches. REMAINING: revive the dead `SemanticError` kind for the MERGE read-own-writes case (Merge1/Merge5, token `MergeReadOwnWrites`).

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [~] 2.1 Documentation — CHANGELOG [3.0.0] updated with the full check list (commit pending). Consider a docs/specs note when 1.4/1.5 land.
- [x] 2.2 Tests — 17 in-module unit tests (positive + negative per check).
- [x] 2.3 Run tests — full nexus-core suite green after each increment (latest 4387/0).
