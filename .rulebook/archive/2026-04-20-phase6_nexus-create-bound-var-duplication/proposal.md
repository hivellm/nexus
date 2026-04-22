# Proposal: fix Nexus CREATE over-creating bound-variable node references

## Why

Discovered during verification of `phase6_nexus-delete-executor-bug`
on 2026-04-20 (diagnostic probe against a live RPC listener, before
the DELETE fix landed in `d46e2cfc`).

A single `CREATE` statement that declares node variables and then
references those variables in a relationship pattern *inside the
same CREATE* over-creates the referenced nodes. Reproducer:

```cypher
CREATE (a:X {id: 1}), (b:X {id: 2}), (a)-[:R]->(b)
```

Expected result on `MATCH (n) RETURN count(n)`: 2.
Observed: **4**.

The two unexpected nodes are unbound `(:X)` nodes without the
`{id: ...}` properties — Nexus treats `(a)` and `(b)` in the edge
pattern as **new, anonymous nodes** instead of binding to the
variables declared earlier in the same CREATE.

This is Neo4j-incompatible behaviour: Neo4j (verified against
2025.09.0 community) creates exactly 2 nodes + 1 relationship for
the same statement. It also inflates the `TinyDataset` in
`crates/nexus-bench/src/dataset.rs` from 100 to 200 nodes on
Nexus — every `(nX)-[:KNOWS]->(nY)` in the edge section of the
load literal re-creates `nX` and `nY` as unbound duplicates.

The scenarios in the seed catalogue do not catch this bug because
they filter by id or label (`MATCH (n {id: 42})`,
`MATCH (n:A)`) and the duplicates have no `id` / no matching
label beyond `X`, so the bench ratio comparisons still look
consistent. The bug surfaces only on total-count assertions like
`phase6_bench-live-test-state-isolation`'s isolation tests.

## What Changes

### Diagnosis

- Isolate the layer: parser, planner, or executor CREATE operator.
  The AST almost certainly carries a single CREATE pattern with
  variable bindings that resolve to the earlier declarations —
  the ambiguity lives downstream.
- Compare Nexus vs Neo4j on a Neo4j-compatible baseline: does the
  `CREATE a, b, (a)-[:R]->(b)` pattern variant work? What about
  `CREATE (a:X), (b:X) WITH a, b CREATE (a)-[:R]->(b)` (two
  clauses)?

### Fix

- Locate the CREATE pattern-walker and ensure variable references
  in relationship patterns bind to the earlier node declarations
  in the same CREATE scope. Plain parser / planner choice; no
  speculation on the exact layer here until §1 nails it down.

### Tests

- Direct unit test in `crates/nexus-core/src/executor` with the
  minimal reproducer above, asserting `count == 2` after CREATE.
- Integration test that loads the bench's `TinyDataset` and
  asserts exactly 100 nodes + 50 relationships post-load.
- Strengthen `phase6_bench-live-test-state-isolation`'s
  `isolation_between_loads_works` + `isolation_between_tests_works`
  to assert `count == 100` after each load once this bug is
  fixed (currently they only assert the reset contract, not the
  load count, to stay green under the bug).

## Impact

- Affected specs: `create-pattern-binding`,
  `bench-state-isolation` (will regain the full load-count
  assertion).
- Affected code:
  - `crates/nexus-core/src/executor/planner/queries.rs` — CREATE
    pattern handling
  - `crates/nexus-core/src/executor/operators/create.rs` (or
    wherever the create operator lives) — variable-binding
    resolution across the single CREATE statement
  - `crates/nexus-bench/tests/live_*.rs` — strengthen isolation
    assertions once the fix ships
- Breaking change: NO — restores documented openCypher semantics.
  Callers that somehow relied on the duplicate-unbound behaviour
  are not relying on anything a real openCypher engine
  guarantees.
- User benefit: closes a real data-integrity gap (a CREATE
  statement silently creates twice as many nodes as the source
  says), unblocks a stricter contract on the bench isolation
  tests, and restores `TinyDataset` to its documented 100-node
  / 50-edge shape on the Nexus side of a comparative run.

Source: surfaced during `phase6_nexus-delete-executor-bug`
verification.
