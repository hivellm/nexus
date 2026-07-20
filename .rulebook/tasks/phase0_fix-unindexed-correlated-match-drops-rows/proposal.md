# Proposal: phase0_fix-unindexed-correlated-match-drops-rows

**Priority: CRITICAL — an `UNWIND … MATCH (a:Label {prop: r.field})` whose
`(Label, prop)` pair has NO property index silently returns only the FIRST
driving row's matches and drops every later driving row.** Found while writing
discriminating verification tests for `phase0_fix-correlated-predicate-index-seek`;
not previously reported.

## Why

A correlated inline property predicate whose value comes from an `UNWIND` row
(`MATCH (a:P {id: r.s})`) takes one of two execution paths:

- **Indexed** (`:P(id)` has an index): plans a per-row `NodeIndexSeek`. This is
  correct as of `phase0_fix-correlated-predicate-index-seek` (returns every
  match for every driving row).
- **Unindexed** (no index on `:P(id)`): plans `[Unwind, NodeByLabel(a),
  Filter(a.id = r.s), …]` — a full label scan cross-joined with the driving
  rows, then a residual filter. **This path returns wrong results.**

### Confirmed empirically (Nexus 3.0.0, release/3.0.0)

Nodes: `:P {id: 10}`, `:P {id: 20}`, `:P {id: 30}`, `:P {id: 20}` (id 20 twice).
No index on `:P(id)`.

```cypher
UNWIND [{s: 10}, {s: 20}, {s: 99}, {s: 30}] AS r
MATCH (a:P {id: r.s})
RETURN a.id
```

| Path | Result |
|---|---|
| indexed (reference — `NodeIndexSeek`) | `[10, 20, 20, 30]` correct |
| **unindexed (`NodeByLabel` + `Filter`)** | **`[10]`** BUG |

The unindexed path keeps only the matches for the FIRST driving row and drops
the rest — including `30`, which is a unique single match. Both a genuine miss
(`99`) and every subsequent hit vanish.

### Mechanism — HYPOTHESIS (to confirm in §1/§2, not yet root-caused)

The plan order is correct (`NodeByLabel` is bucketed as a scan in
`optimize_operator_order`, so the `Filter` runs AFTER `a` is bound — this is NOT
the `phase0_fix-plan-reorder-drops-predicates` bucketing bug). The defect is in
how the residual `Filter (a.id = r.s)` — a synthesized predicate string that is
re-parsed and evaluated — handles a **correlated right-hand side** (`r.s`, which
varies per driving row) after the `UNWIND × NodeByLabel` cross product. Leading
hypotheses: the filter binds `r.s` once (to the first row's value) rather than
per row, or the cross-product/materialisation collapses the driving rows. §1
must reproduce and §2 must pinpoint the site before any fix.

## What Changes

- Reproduce the truncation with a failing test over an unindexed
  `UNWIND … MATCH (a:P {prop: r.field})` (§1).
- Diagnose whether the fault is in the residual filter's evaluation of a
  correlated RHS, in the `UNWIND × NodeByLabel` cross-product seeding, or in
  materialisation — pinpoint the exact site (§2).
- Fix so the unindexed path returns the SAME rows as the indexed seek path (the
  `phase0_fix-correlated-predicate-index-seek` reference): every match for every
  driving row (§3).

## Impact

- Affected specs: none directly; changes executor filter/cross-product semantics
- Affected code: `crates/nexus-core/src/executor/` — the residual `Filter`
  evaluation (`operators/filter.rs`), the scan seeding (`dispatch.rs`
  `seed_scan_main_loop`), and/or the inline-property `Filter` synthesis in
  `planner/queries/strategy.rs`
- Breaking change: NO — turns silently-wrong results into correct ones
- User benefit: correlated batch reads/writes by natural key return correct
  results even without an index (just slower than the indexed seek)
- Related: `phase0_fix-correlated-predicate-index-seek` (fixed the indexed path;
  this is the unindexed fallback), `phase0_fix-plan-reorder-drops-predicates`
  (same `optimize_operator_order`, different defect — ruled out here),
  `phase0_fix-where-predicate-reparse-precedence` (residual-predicate re-parse is
  a candidate site), `phase0_fix-where-clause-index-seek` (the WHERE form of the
  same shape)
