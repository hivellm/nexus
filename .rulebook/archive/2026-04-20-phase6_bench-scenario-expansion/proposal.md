# Proposal: Broaden the nexus-bench scenario catalogue

## Why

The rebuilt harness (parent task
`phase6_nexus-vs-neo4j-benchmark-suite`) shipped with 9 seed
scenarios covering scalar / point-read / label-scan / aggregation
/ filter / order. That's enough to prove the plumbing works and
sanity-check regressions at the ceiling, but it does not exercise
the features every Phase 6 implementation task cares about:
traversals, writes, indexes, constraints, subqueries, procedures,
temporal / spatial, and hybrid RAG-style queries.

The parent task deliberately parked these categories because each
one needs the `tiny` dataset to grow just enough to make the
scenario meaningful — not a 10k-node fan-out, but slightly more
structure than 100 nodes + sequential `KNOWS` edges. Scoping the
catalogue expansion separately means each category lands as its own
reviewable diff without bloating the original task.

Nexus-only measurement is the target. The comparative Neo4j side is
tracked in `phase6_bench-neo4j-docker-harness`; this task is
independent and ships value immediately (regression detection for
the Phase 6 feature rollouts).

## What Changes

### Dataset expansion

- Keep `TinyDataset` as-is (100 nodes, single-CREATE). Guard rails
  from the parent task — no fan-out loaders — still apply.
- Add a `small` dataset (≤ 500 nodes, ≤ 3 KiB literal) for the
  traversal / index categories that need a richer graph. Still a
  single CREATE; still hardcoded as a static string.
- Add a `vector_small` dataset (50 nodes × 16-dim `score_vec`
  property) for the KNN / hybrid scenarios, once the core vector
  indexes support sub-100-node KNN cleanly.

### Scenario catalogue

Grouped by the parent task's §10–§17 numbering. Each category
lands ~5–10 scenarios:

- **§10 Traversals**: 1-hop / 2-hop / variable-length `*1..3` /
  QPP `{1,5}` / shortestPath / cartesian-join MATCH.
- **§11 Writes**: single CREATE / UNWIND batch / MERGE / SET / SET
  += / DELETE / `CALL {} IN TRANSACTIONS`.
- **§12 Indexes**: bitmap label scan / B-tree seek / composite
  prefix / KNN k=1/10 / R-tree `withinDistance` / full-text
  single-term.
- **§13 Constraints**: UNIQUE / NOT NULL / NODE KEY / property-type
  insert-overhead measurements.
- **§14 Subqueries**: `EXISTS` / `COUNT` / `COLLECT` / nested 3-deep
  / `CALL {} IN TRANSACTIONS` throughput.
- **§15 Procedures**: `db.labels` / `db.indexes` / `apoc.coll.*` /
  `apoc.map.*` / `apoc.path.expand` / `gds.pageRank`.
- **§16 Temporal & Spatial**: `date.*` / `point.distance` /
  R-tree seek.
- **§17 Hybrid**: vector+graph / full-text+vector / temporal+spatial
  compounds.

Each scenario declares its `expected_row_count` so the divergence
guard keeps catching regressions in row-shape, not just latency.

### Tests

- Unit tests verify catalogue invariants (unique ids, non-zero
  expected rows, category prefix convention) keep passing as new
  entries land.
- New category tests live alongside the scenario they exercise —
  e.g. `tests/traversals.rs` opt-in via `live-bench`, still
  `#[ignore]` by default.

## Impact

- Affected code:
  - `crates/nexus-bench/src/dataset.rs` — add `SmallDataset` +
    `VectorSmallDataset` constants.
  - `crates/nexus-bench/src/scenario_catalog.rs` — grow from ~9 to
    ~60 scenarios, split into category-named submodules under
    `src/scenarios/`.
  - `crates/nexus-bench/tests/` — per-category integration tests,
    all `#[ignore]`.
- Breaking change: NO — additive. Existing scenario ids stay.
- User benefit: regression gate covers every Phase 6 feature
  (QPP, FTS, APOC, advanced types, …) as they land, instead of
  shipping those features blind on latency terms.
