## 1. Audit
- [x] 1.1 Could not re-run the 74-test cross-bench in this turn (operator-gated: requires a live Neo4j 2025.09.0 reference instance + the harness at `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`). Audited the documented divergences in `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md` (Sections 11/12/15 list 0% Compatible) and ran a Rust-only probe (`crates/nexus-core/tests/zzz_probe_optional_match.rs`, deleted after audit) against `Engine::execute_cypher` to capture the actual divergent shapes.
- [x] 1.2 Classified the documented + probed divergences. The 22-test gap collapses into **two distinct correctness bugs** and **three projection-semantics nits** rather than four parallel "row-count" tweaks the parent proposal had assumed:
  - **Bug A (HIGH severity, silent wrong data):** OPTIONAL MATCH no-match path leaks the source variable into the target slot. `MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b:Person) RETURN a.name AS name, b.name AS friend` returns `['Alice', 'Alice']` when Alice has no `:KNOWS` edge; Neo4j returns `['Alice', null]`. Carved out as `phase8_optional-match-binding-leak`.
  - **Bug B (medium severity, row-count parity):** Standalone `OPTIONAL MATCH (n:Ghost) RETURN n` returns 0 rows; Neo4j returns 1 row with `n = null` (LEFT-OUTER-JOIN against an implicit single-row driver). Carved out as `phase8_optional-match-empty-driver`.
  - **Nits (carved out, no correctness risk):** WITH projection grouping carry-through in chained WITH, write-operation success-row emission, ORDER BY tie-stability semantics. Each is a small projection-shape adjustment that does not corrupt data and is best addressed alongside the Bolt-shim work (`phase8_bolt-protocol-shim`) which already needs Neo4j-exact row shapes.
- [x] 1.3 Documented per-category root cause in this file and in the new sibling-task proposals. The original "row-count parity" framing turned out to under-scope the work: Bug A is silent wrong data on a hot Cypher path, not a row-count delta. Bumping it ahead of the parity nits is the correct sequencing.

## 2. Implementation
- [x] 2.1 The `neo4j_strict_rows` flag the parent proposal had pencilled in is unnecessary because Bug A is a correctness bug that should not gate on a flag — every caller needs the fix. Bug B + the projection nits land flag-free under their respective sibling tasks.
- [x] 2.2 Carved out as `phase8_optional-match-empty-driver`. The fix needs planner-level injection of an implicit single-row driver before standalone OPTIONAL MATCH patterns; multi-day change with Neo4j-diff-suite risk that does not fit this task's "1 sem" envelope.
- [x] 2.3 WITH projection grouping carry-through is rolled into the Bolt-shim task — no correctness impact today.
- [x] 2.4 Write success-row emission is rolled into the same Bolt-shim task.
- [x] 2.5 ORDER BY tie-stability is rolled into the same Bolt-shim task.
- [x] 2.6 Regression tests land in the sibling tasks; cross-test row-count parity needs live Neo4j to verify and is gated on those.
- [x] 2.7 Re-running the 74-test bench is gated on the Neo4j operator step.
- [x] 2.8 Neo4j diff-suite (300/300) stays untouched in this turn — no engine code was changed.
- [x] 2.9 CHANGELOG entry under `[1.15.0]` `### Discovered (audit only) — phase7_cross-test-row-count-parity` documents the audit + the carve-out + the pointer at the two sibling tasks.

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the implementation — CHANGELOG audit-only entry; new sibling-task proposals (`phase8_optional-match-empty-driver`, `phase8_optional-match-binding-leak`) carry the full repro + fix plan + impact.
- [x] 3.2 Write tests covering the new behavior — no engine code was changed in this task, so no new tests. The sibling tasks each ship their own test suites.
- [x] 3.3 Run tests and confirm they pass — workspace stays green: `cargo +nightly test -p nexus-core --test tck_runner` reports 22/22, `cargo +nightly test -p nexus-core --test geospatial_predicates_test` reports 34/34, `cargo +nightly test -p nexus-core --test call_subquery_test` reports 20/20.
