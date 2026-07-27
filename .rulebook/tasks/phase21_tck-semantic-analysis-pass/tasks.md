## 1. Implementation

Static semantic-analysis pass shipped: `executor/semantic_validation.rs`, hooked
in `engine/query_pipeline.rs::execute_cypher_with_context` (after parse, before
plan). Conservative by design (over-collects binders → never a false positive;
bails on UNION/CALL{}/CALL-proc/LOAD CSV/DDL). 8 openCypher detail tokens across
7 checks, 19 in-module unit tests, full nexus-core suite green after every
increment (final 4389/0).

- [x] 1.1 variable scoping validation stage — 3 checks landed.
  - [x] `UndefinedVariable` (commit 0ac36d07) — reference bound nowhere; excludes the `__DISTINCT__` aggregate marker; bare RETURN/WITH items are references not binders.
  - [x] `VariableTypeConflict` (commit 43bd0325) — a name used as both a node and a relationship.
  - [x] `VariableAlreadyBound` (commit 38b0e806) — CREATE re-declaring a bound var with labels/props (incl. intra-CREATE order); bails on WITH for exact scope.
  - DEFERRED (concrete blockers, tracked for a follow-up slice): `NoVariablesInScope` is unreachable — the parser rejects `RETURN *`/`WITH *` outright, so it never reaches the pass (depends on `RETURN *` parser support); bare standalone `CREATE (a)` (Create1[13]), MATCH/MERGE re-binding (Match6, Merge1/Merge5), and WITH-scope-narrowing.
- [x] 1.2 aggregation placement validation — 2 checks landed.
  - [x] `NestedAggregation` (commit 4d11d060) — aggregate nested in an aggregate (`count(count(*))`).
  - [x] `InvalidAggregation` (commit 4d11d060) — aggregate inside a WHERE.
  - DEFERRED: `AmbiguousAggregationExpression` (implicit-grouping-key analysis — higher false-positive risk).
- [x] 1.3 SKIP/LIMIT arg validation (commit 7500cea2) — `NegativeIntegerArgument` + `NonConstantExpression`. Runtime negative-parameter case is not statically detectable (deferred to a runtime check).
- [x] 1.4 projection/UNION/write structural validation — `ColumnNameConflict` (commit dd9e0655, duplicate RETURN/WITH aliases) landed. `InvalidDelete` is NOT AST-detectable here — the parser reduces DELETE items to bare identifiers, dropping any `:Label`/`.prop` suffix. UNION structural checks (`DifferentColumnsInUnion`, `InvalidClauseComposition`) are DEFERRED — they require removing the pass's UNION bail-out, a larger change.
- [x] 1.5 OpenCypherErrorKind + detail tokens emitted — every check emits `Error::CypherSyntax("<Token>: …")`, classified as `SyntaxError`, carrying the CamelCase token the TCK runner substring-matches. DEFERRED: reviving the dead `SemanticError` kind for the MERGE read-own-writes case (Merge1/Merge5, token `MergeReadOwnWrites`), a runtime check.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Documentation — CHANGELOG [3.0.0] check list (commit def7016f) + `docs/specs/cypher-subset.md` § Semantic Validation token table + deferred list (commit dd9e0655).
- [x] 2.2 Tests — 19 in-module unit tests (positive + negative per check).
- [x] 2.3 Run tests — full nexus-core suite green after each increment (final 4389/0).
