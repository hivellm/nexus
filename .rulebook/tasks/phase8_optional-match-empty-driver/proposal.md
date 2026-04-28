# Proposal: phase8_optional-match-empty-driver

Source: carved out of `phase7_cross-test-row-count-parity` after a
probe found this is one of two distinct OPTIONAL MATCH correctness
bugs that the parent's "row-count parity" framing under-scoped.
The parent could not close cleanly without splitting these out.

## Why

Probe `cargo +nightly test -p nexus-core --test
zzz_probe_optional_match` (deleted after the audit, output captured
in `phase7_cross-test-row-count-parity` archive) showed:

- `OPTIONAL MATCH (n:Ghost) RETURN n` → **0 rows** (Nexus today)
- Neo4j's contract on the same query → **1 row with `n = null`**

OPTIONAL MATCH is a LEFT OUTER JOIN against an implicit single-row
driver when there is no prior MATCH clause. Nexus's planner today
emits a regular `NodeByLabel + Project` plan that produces zero
rows when the labelled set is empty, instead of falling through to
a wrapped-NULL row. This breaks every Neo4j-targeted client that
expects "OPTIONAL = always at least one row" semantics for
standalone optional patterns and contributes the largest single
class of row-count divergence in the 74-test cross-bench
(`docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md` Section 11
"OPTIONAL MATCH" — 0/2 compatible).

## What Changes

- Planner-level: detect "first clause is OPTIONAL MATCH and there
  is no prior driving table". Inject an implicit `ImplicitDriver`
  (or `SingleEmptyRow`) source operator before the OPTIONAL match
  pattern so the LEFT-OUTER-JOIN semantics already in
  `Operator::Expand { optional: true }` and
  `OptionalFilter` apply.
- `Operator::NodeByLabel` and `AllNodesScan` may need an
  `optional: bool` flag (or a new `Operator::OptionalNodeByLabel`)
  so the executor knows to emit a single NULL-bound row when the
  scan produces no matches.
- Tests: pin the contract end-to-end against `Engine::execute_cypher`:
  - `OPTIONAL MATCH (n:Ghost) RETURN n` → 1 row, `n = null`.
  - `OPTIONAL MATCH (n:Ghost) RETURN n.name AS name` → 1 row,
    `name = null`.
  - `OPTIONAL MATCH (n:Ghost) RETURN count(n) AS c` → 1 row,
    `c = 0` (already passes).
  - Regression: `MATCH (a:Ghost) OPTIONAL MATCH (a)-[:KNOWS]->(b)
    RETURN a, b` stays at 0 rows (the prior MATCH already
    eliminated all rows; OPTIONAL MATCH after a real MATCH does
    not re-introduce wrapped-NULL rows).
- Confirm Neo4j diff suite stays at 300/300; expand the cross-bench
  coverage of Section 11 (OPTIONAL MATCH) by a few scenarios.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` OPTIONAL MATCH section
  to document the implicit-driver semantics.
- Affected code: `crates/nexus-core/src/executor/planner/queries.rs`
  (detect + inject implicit driver), possibly
  `crates/nexus-core/src/executor/types.rs` (operator variant flag),
  `crates/nexus-core/src/executor/operators/{node_scan,expand}.rs`.
- Breaking change: small — clients that depend on Nexus returning
  zero rows for standalone OPTIONAL MATCH-no-match would see one
  wrapped-NULL row. Document in CHANGELOG.
- User benefit: closes the largest single row-count divergence
  class vs Neo4j; drop-in compatibility with Neo4j-targeted client
  code expecting OPTIONAL semantics.

## Source

- Parent task: `phase7_cross-test-row-count-parity` (blocked +
  archived after audit found two distinct correctness bugs needing
  separate tasks).
- Sibling: `phase8_optional-match-binding-leak` covers the second
  bug (Expand binds source variable into target slot when no
  relationship matches).
- Bench evidence:
  `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md` Section 11.
