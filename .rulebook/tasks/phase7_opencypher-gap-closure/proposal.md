# Proposal: phase7_opencypher-gap-closure

## Why
Nexus is NOT at 100% openCypher compatibility. Verified status (2026-07-19 analysis):

- The advertised "300/300 (now 325/325) tests passing" is the **Neo4j 2025.09.0 diff suite**, not the openCypher TCK. There is **no vendored upstream openCypher TCK and no formal pass rate** — only a Nexus-authored spatial Gherkin corpus (22 scenarios, `crates/nexus-core/tests/tck/spatial/`).
- Real openCypher coverage estimate: **~65–70%** (clauses/functions broadly done; gaps in TCK measurement, a few grammar/expression forms, and execution of already-parsed features).

Language-level gaps NOT covered by any existing task:
1. **openCypher TCK integration** — no upstream TCK vendored, no runner, no baseline pass rate. Without it, "100% openCypher" is unmeasurable.
2. **Dynamic labels/types on the read side** — `MATCH (n:$label)` / `-[r:$type]->` with parameter; write-side (`CREATE (n:$x)`, `SET n:$x`) already shipped in phase6_opencypher-advanced-types, read-side parser only accepts string literals (`crates/nexus-core/src/executor/parser/ast.rs`, Pattern).
3. **UNION inside CALL { } subqueries** — parser rejects `CALL { ... UNION ... }` (`ast.rs` CallSubqueryClause).
4. **`CREATE CONSTRAINT ... FOR ... REQUIRE` DDL form** — marked Unsupported in `docs/specs/cypher-subset.md` (~line 1562); constraint semantics exist, the openCypher/Neo4j 5+ surface syntax does not.

Gaps already tracked elsewhere (this task depends on, but must NOT duplicate them):
- QPP slice 2 (named/labelled bodies) → `phase6_opencypher-quantified-path-patterns`
- `CALL { } IN TRANSACTIONS` executor batching → `phase6_opencypher-subquery-transactions`
- Geospatial predicate execution → `phase6_opencypher-geospatial-predicates`
- `USING INDEX/SCAN/JOIN` hint enforcement → `phase7_planner-using-index-hints`

## What Changes
- Vendor the official openCypher TCK (Gherkin feature files) and build a Rust runner (extend the existing `tests/tck/spatial` harness) producing a per-category pass-rate report committed under `docs/compatibility/`.
- Implement read-side dynamic labels/rel-types (`:$param`) end-to-end: parser → planner (label resolution at execution time) → executor.
- Allow `UNION` / `UNION ALL` inside `CALL { }` subqueries: parser + planner wiring to the existing Union operator.
- Add `CREATE CONSTRAINT ... FOR (n:L) REQUIRE n.p IS UNIQUE | IS NOT NULL` grammar mapped to the existing constraint machinery.
- Update `docs/specs/cypher-subset.md`, `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`, and CLAUDE.md's compatibility claim with the measured TCK number (replace the stale "~55%").

## Impact
- Affected specs: docs/specs/cypher-subset.md; docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md
- Affected code: crates/nexus-core/src/executor/parser/ (ast.rs, mod.rs); crates/nexus-core/src/executor/planner/; crates/nexus-core/src/executor/operators/ (expand, union/call_subquery); crates/nexus-core/src/catalog/ (runtime label/type resolution); crates/nexus-core/tests/tck/
- Breaking change: NO (purely additive grammar + a new test harness)
- User benefit: honest, measurable openCypher conformance number; queries written for Neo4j 5+/openCypher (dynamic labels, CALL+UNION, FOR...REQUIRE) run unmodified on Nexus.
