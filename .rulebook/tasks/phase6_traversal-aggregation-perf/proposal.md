# Proposal: phase6_traversal-aggregation-perf

## Why

Bottlenecks #3, #4, #5 in
[docs/nexus/03-performance.md](../../../docs/nexus/03-performance.md) — the
persistent, measured gaps vs Neo4j across every benchmark generation:

- Relationship traversal: single-hop **41–57% slower**; no relationship-type
  pre-filter before walking the adjacency linked list — every hop touches
  full 48-byte records regardless of type selectivity.
- Aggregation: COUNT **44.7% slower**, GROUP BY **39.3% slower**; no
  metadata shortcut for unfiltered `COUNT(*)`/`count(n)` even though the
  catalog + label bitmaps already know the cardinality; GROUP BY uses an
  unsized HashMap.
- Planner: fixed-constant cost model (Expand=100, Join=200); a
  `StatisticsCollector` exists in `executor/optimizer.rs` but is not wired
  into join ordering — JOIN-shaped queries run 43–61% slower.

## What Changes

- Relationship-type pre-filter in the traversal hot path (skip records whose
  `type_id` doesn't match before property/record materialization).
- `COUNT(*)` / `count(n)` with no predicate answered from label-bitmap
  cardinality (~O(1)); mixed cases fall back to scan.
- GROUP BY hash-map pre-sizing from upstream cardinality estimates.
- Wire `StatisticsCollector` outputs (label/type cardinalities, degree
  averages) into join-order and Expand-direction decisions in the planner.

## Impact

- Affected specs: specs/perf/spec.md (this task)
- Affected code: `crates/nexus-core/src/executor/operators/` (expand,
  aggregate), `crates/nexus-core/src/executor/planner/queries/cost.rs`,
  `crates/nexus-core/src/executor/optimizer.rs`
- Breaking change: NO (same results, faster)
- User benefit: closes the headline vs-Neo4j gaps in traversal and
  aggregation; complex-query plans stop being cardinality-blind.

## Success criteria (gate)

nexus-bench before/after: rel-traversal gap cut ≥30%; unfiltered COUNT(*)
constant-time regardless of node count; GROUP BY ≥25% faster at 100k rows;
no regression elsewhere in the suite.
