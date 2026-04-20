# Proposal: close the correctness gaps `nexus-bench --compare` surfaced

## Why

The first full comparative run of the bench harness on
2026-04-20 (against a release `nexus-server` + docker
`neo4j:latest` on Bolt :7687) produced the expected
latency-domination story — Nexus leads in 23 / 28 scenarios,
parity in 3, ⚠️ behind in 2, no 🚨 gap — but the
content-divergence guard (commit `8a790d06`) flagged **eight
scenarios where Nexus's answer is wrong**. Latency ratios on
those scenarios compare apples to oranges: a 0.53× on
`traversal.small_one_hop_hub` is meaningless when Nexus returns
99 and Neo4j returns 5.

This task tracks the fixes for those eight divergences. As each
lands, the bench is re-run, the numbers in this file's *current
state* section move from "wrong" to "matched", and eventually
the full catalogue runs content-clean.

## Progress log (partial landing, 2026-04-20)

Not every bug in the catalogue landed in one pass — §1, §2, §3, §5
carry dependencies into other subsystems that made "land the narrow
fix here" unsafe without a wider refactor. The in-pass punch list:

- **§4 Integer arithmetic** — **DONE**. `crates/nexus-core/src/executor/eval/arithmetic.rs`
  now runs an integer-preserving fast path (with `checked_*` overflow
  → f64 fallback) across add/subtract/multiply/divide/modulo.
  Regression test: `integer_only_arithmetic_stays_integer` (`crates/nexus-core/src/engine/tests.rs`).
- **§7 ORDER BY null polarity** — **DONE**. `execute_sort` /
  `execute_top_k_sort` now consult a `cypher_null_aware_order` helper
  that places NULLs last in ASC, first in DESC, without changing the
  base predicate comparator (so `<` / `>` / WHERE evaluation keep
  their existing contract). Regression test:
  `order_by_null_positioning_matches_opencypher`. Caveat: the bench
  query shape `RETURN n.name ORDER BY n.score DESC LIMIT 5` also
  needs the sort-column-not-in-projection resolution, which is a
  separate, pre-existing planner bug and not part of §7 — the bench
  scenario will still diverge until that is closed. Regression test
  projects `n.score` explicitly to isolate the null-polarity fix.
- **§8 DELETE after CREATE (engine-level)** — **PARTIALLY DONE**. The
  engine's "DELETE requires MATCH clause" check at
  `crates/nexus-core/src/engine/mod.rs` now accepts CREATE-bound
  variables too; `CREATE (n:BenchCycle) DELETE n` executes. The
  bench's full `CREATE ... WITH n DELETE n RETURN ...` form hits a
  separate **parser** limitation around WITH followed by DELETE —
  tracked as a follow-up.
- **§9 Statistical aggregations** — **DONE**. Planner's
  `contains_aggregation`, the inline-aggregation matchers in both
  the no-pattern and MATCH+RETURN paths, and the post-agg wrapper
  matcher all now recognise `stdev`, `stdevp`, `percentileCont`,
  `percentileDisc` as aggregations so they collapse the row set.
  Regression test: `statistical_aggregations_collapse_to_one_row`.

Not landed this pass (still open):

- (engine-level work closed for this task; only §10 bench re-run
  against a live Neo4j remains.)

Added to "DONE" after the third pass (2026-04-20, same day):

- **§5.3 `WITH ... WHERE ... RETURN count(*)`** — **DONE**. The
  WITH-insertion pass at `crates/nexus-core/src/executor/planner/queries.rs`
  now treats `Operator::Aggregate` as a valid sink alongside
  `Operator::Project`. Before this change, the insertion looked only
  for `Project` and appended WITH at the end of the operator list
  when none was found — which meant `MATCH (n:A) WITH n.score AS s
  WHERE s > 0.1 RETURN count(*)` landed WITH (and its Filter) AFTER
  Aggregate. WITH's projection then ran on already-collapsed rows,
  Filter dropped everything, and the final row set was empty. With
  Aggregate treated as a sink, WITH + Filter insert before it and
  the pipeline runs in the correct Cypher order. Regression test:
  `with_projection_and_filter_run_before_return_aggregation`.
- **§8.2 `CREATE ... WITH ... DELETE ... RETURN <expr>`** — **DONE**.
  Root cause was NOT a parser bug: the full shape
  `CREATE (n:Phase6_82) WITH n DELETE n RETURN 'done' AS status`
  parses on the first pass. The DELETE+RETURN branch in
  `execute_cypher_ast` (`crates/nexus-core/src/engine/mod.rs`) detected
  `is_count_only == false` and round-tripped the AST through
  `query_to_string`, which emits `format!("{:?}", clause)` — Rust
  debug output, not Cypher — and the executor's re-parse then failed
  with `Expected identifier` at column 40 of that gibberish. Fix:
  install the RETURN tail as a `preparsed_ast_override` on the
  executor so the executor consumes the AST directly, skipping the
  re-parse entirely. Regression test:
  `create_with_delete_return_parses_and_executes`.

- **§1 Composite `:Label {prop}` filter** — **DONE**. Three coordinated
  edits in `crates/nexus-core`:
  1. **Planner** (`executor/planner/queries.rs`) —
     `synthesise_anonymous_source_anchors` assigns a synthetic
     `__anchor_<n>` variable to anonymous source nodes that carry
     labels or properties. The subsequent NodeByLabel + Filter pair
     constrains the source set, and `add_relationship_operators`
     resolves the Expand's `source_var` to the synthesised name
     instead of leaving it empty.
  2. **Expand** (`executor/operators/expand.rs`) — the source-less
     fallback now fires only when `source_var.is_empty()`.
     Previously it also fired when the declared `source_var` had no
     rows (because NodeByLabel returned empty), silently turning
     "anchor matched zero nodes" (correct = 0) into "scan every
     relationship" (8-row over-count). This guard is independent of
     §1's planner fix but necessary for it to hold correctness when
     NodeByLabel legitimately returns zero.
  3. **CREATE** (`executor/operators/create.rs`) — label-index
     rebuild no longer reverse-engineers label IDs from
     `NodeRecord.label_bits`. A new `created_nodes_with_labels`
     list is populated during node creation and fed directly to
     `label_index.add_node` post-commit. The old rebuild loop
     iterated `for bit in 0..64`, dropping any `label_id >= 64`
     — a real bug once the catalog accumulates enough labels. In
     the test suite (shared catalog across parallel tests) :P gets
     label_id = 134; this fix lets `NodeByLabel(134)` return the
     actual :P nodes instead of an empty bitmap.
  Regression lock:
  `match_anonymous_anchor_with_label_and_property_scopes_expand`
  (un-ignored), `match_anonymous_anchor_var_length_expansion_is_bounded_by_filter`
  (new), and `match_scopes_by_label_and_property_together` all green.
- **§2 Variable-length path** — **DONE**. Same root cause as §1
  (anonymous anchor leaves `VariableLengthPath.source_var` empty).
  Same planner fix covers it; no separate operator-level change
  needed because `add_relationship_operators` feeds `source_var`
  into both `Expand` and `VariableLengthPath` from the same
  `prev_node_var`. Regression test:
  `match_anonymous_anchor_var_length_expansion_is_bounded_by_filter`.

Added to "DONE" after the second pass (2026-04-20, same day):

- **§3 `db.*` catalog procedures + `YIELD *`** — **DONE (parser +
  engine-level)**. Parser widened at `parser/clauses.rs:1680` so
  `YIELD *` short-circuits to `yield_columns = None`, which the
  executor already treats as "project every column the procedure
  declares". Engine-level contract for `db.labels()` / `db.propertyKeys()`
  / `db.relationshipTypes()` locked by `db_labels_procedure_emits_a_row_per_label`.
  The bench's "0 row" observation is a distinct RPC-path issue,
  separately tracked.
- **§5 WITH → RETURN projection (2 of 3 scenarios)** — **DONE** for
  `subquery.exists_high_score` and `subquery.size_of_collect`. The
  planner now stashes RETURN's items into `post_aggregation_return_items`
  when WITH carries the aggregation, and emits a final `Project`
  after `Aggregate` (inserted before any `Limit`). Locked by
  `with_aggregation_then_return_expression_projects_correctly`.
  `subquery.with_filter_count` stays open as §5.3.
- **§6 avg() float-accumulation** — **DOCUMENTED-AS-ACCEPTED**. The
  divergence is two ULPs on the 15th decimal place, well below any
  user-facing assertion; the bench's strict-equality guard is the
  only place this matters. Direction (a) "Kahan summation" is the
  right long-term fix, but the gate decision is (c) here: accept the
  informational divergence and let the bench's tolerance catch up in
  a follow-up.

## Bench runs (append-only log; one entry per re-run)

Snapshots live under
`docs/benchmarks/baselines/`. Every row references the exact
Nexus commit the bench ran against so a latency regression
(or a fix) has a commit to point at.

### Run 8 — 2026-04-20 · Nexus commit `34f38c87` · 51 catalog / 51 ran

Second clean-Neo4j re-run (same `wipe → fresh server → bench` recipe).
Confirms Run 7's classification held and improved:

| Bucket | Count | Δ vs Run 7 |
|---|---|---|
| ⭐ Lead | 42 | +1 |
| ✅ Parity | 6 | +1 |
| ⚠️ Behind | 0 | -2 |
| 🚨 Gap | 3 | same |
| — n/a | 0 | same |
| content-divergent (§1-§9) | 0 | same |

§1-§9 rows (unchanged green):
- `traversal.small_one_hop_hub` 428µs / 1730µs = 0.25× ⭐
- `traversal.small_two_hop_from_hub` 683µs / 1677µs = 0.41× ⭐
- `traversal.small_var_length_1_to_3` 281µs / 1746µs = 0.16× ⭐
- `procedure.db_labels` 1483µs / 1603µs = 0.93× ✅
- `procedure.db_relationship_types` 1542µs / 1581µs = 0.98× ✅
- `procedure.db_property_keys` 110µs / 1699µs = 0.06× ⭐
- `scalar.arithmetic` 96µs / 1491µs = 0.06× ⭐
- `subquery.exists_high_score` 538µs / 1848µs = 0.29× ⭐
- `subquery.size_of_collect` 184µs / 1723µs = 0.11× ⭐
- `subquery.with_filter_count` 206µs / 1707µs = 0.12× ⭐
- `aggregation.avg_score_a` 157µs / 1603µs = 0.10× ⭐
- `aggregation.stdev_score` 160µs / 1600µs = 0.10× ⭐
- `order.top_5_by_score` 572µs / 1500µs = 0.38× ⭐
- `order.bottom_5_by_score` 564µs / 1585µs = 0.36× ⭐
- `write.create_delete_cycle` 411µs / 1710µs = 0.24× ⭐

Three 🚨 Gaps (all pre-existing perf, not correctness):
- `traversal.cartesian_a_b` 551513µs / 1686µs = 327× (cartesian materialisation)
- `constraint.not_null_set` 3571µs / 1615µs = 2.21×
- `write.set_property` 3575µs / 1667µs = 2.14×

Out-of-scope content divergences unchanged from Run 7 (QPP, shortestPath,
EXISTS{}, temporal/spatial, COUNT{} subquery, UNWIND-before-CREATE).

Snapshot: `docs/benchmarks/baselines/2026-04-20-run8.{md,json}`.

### Run 7 — 2026-04-20 · Nexus commit `edb331bc` · 51 catalog / 51 ran

First bench run with the §1-§9 fixes all landed. Neo4j wiped clean
(`MATCH (n) DETACH DELETE n`) before the run so the Run 6 "Neo4j
has 2× data" artefact is gone and every row compares apples-to-apples.

| Bucket | Count | Delta vs Run 6 |
|---|---|---|
| ⭐ Lead | 41 | +2 |
| ✅ Parity | 5 | +4 |
| ⚠️ Behind | 2 | +1 |
| 🚨 Gap | 3 | +1 |
| — n/a | 0 | -4 |
| content-divergent | 0 (was ~11) | -11 |

Every §1-§9 scenario is now Lead or Parity and content-matching:
- `traversal.small_one_hop_hub` — Nexus 5 / Neo4j 5 — 636µs / 1630µs = 0.39× ⭐ (§1)
- `traversal.small_two_hop_from_hub` — Nexus 5 / Neo4j 5 — 1017µs / 1749µs = 0.58× ⭐ (§1)
- `traversal.small_var_length_1_to_3` — Nexus 15 / Neo4j 15 — 426µs / 1769µs = 0.24× ⭐ (§2)
- `procedure.db_labels` — Nexus 6 / Neo4j 6 — 1416µs / 1580µs = 0.90× ✅ (§3)
- `procedure.db_relationship_types` — Nexus 1 / Neo4j 1 — 1465µs / 1550µs = 0.95× ✅ (§3)
- `procedure.db_property_keys` — Nexus / Neo4j match — 113µs / 1649µs = 0.07× ⭐ (§3)
- `scalar.arithmetic` — Nexus 7 / Neo4j 7 — 102µs / 1548µs = 0.07× ⭐ (§4)
- `subquery.exists_high_score` — Nexus false / Neo4j false — 738µs / 1740µs = 0.42× ⭐ (§5.1)
- `subquery.size_of_collect` — Nexus 20 / Neo4j 20 — 228µs / 1650µs = 0.14× ⭐ (§5.2)
- `subquery.with_filter_count` — Nexus 9 / Neo4j 9 — 274µs / 1602µs = 0.17× ⭐ (§5.3)
- `aggregation.avg_score_a` — ULP drift (documented) — 155µs / 1699µs = 0.09× ⭐ (§6)
- `order.top_5_by_score` + `order.bottom_5_by_score` — both Lead, content-matched (§7)
- `write.create_delete_cycle` — executes cleanly — 500µs / 1669µs = 0.30× ⭐ (§8.1/8.2)
- `aggregation.stdev_score` — Nexus / Neo4j match — 157µs / 1719µs = 0.09× ⭐ (§9)

Remaining content divergences (out of §1-§9 scope — separate tasks):
- `subquery.count_subquery` — Nexus emits Null for COUNT{…} subquery; Neo4j emits the per-row count. Known subquery-expression gap.
- `write.unwind_create_batch` — Nexus returns 1, Neo4j 10 for `UNWIND range(1,10) CREATE(:X) RETURN count(*)`. UNWIND-before-CREATE collapses on Nexus.
- `scalar.duration_between_days`, `scalar.point_distance_cartesian`, `scalar.point_distance_wgs84`, `scalar.point_within_distance` — temporal/spatial built-ins not wired up on Nexus.
- `traversal.small_qpp_1_to_5` — quantified path patterns not implemented.
- `traversal.small_shortest_path_hub`, `subquery.exists_block` — parser rejects `shortestPath` / `EXISTS { … }` block syntax.

Remaining performance gaps (not correctness):
- `traversal.cartesian_a_b` 🚨 — 794708µs / 1752µs = 453× slower. Pre-existing cartesian-product materialisation perf issue, already documented in Run 2.

Snapshot: `docs/benchmarks/baselines/2026-04-20-run7.{md,json}`.


### Run 6 — 2026-04-20 · Nexus commit `5caef298` · 49 catalog / 43 ran

Catalogue grew to 49 after `scalar.date_literal`,
`scalar.duration_between_days`, `scalar.point_distance_cartesian`,
`scalar.point_distance_wgs84`, `procedure.dbms_components`,
`write.unwind_create_batch`, `subquery.count_subquery` landed —
filling the §9 (Temporal/Spatial) and §8 (procedures) rows of
the scenario-expansion task.

| Bucket | Count |
|---|---|
| ⭐ Lead | 39 |
| ✅ Parity | 1 |
| ⚠️ Behind | 1 |
| 🚨 Gap | 2 |
| content-divergent | ~11 |
| bench-aborting errors | 6 |

New findings (all consolidated here instead of growing the
§ list further — each new scenario maps to an existing root
cause or sits on a class the task already tracks):

- `scalar.duration_between_days`, `scalar.point_distance_cartesian`,
  `scalar.point_distance_wgs84` — Nexus returns **0 rows** on
  every temporal / spatial built-in bench-run. Either the
  functions are not registered in the evaluator, or they
  throw silently and the RPC path swallows the error. New
  class of gap, noted here; would get its own § when it gets
  attention.
- `procedure.dbms_components` — Nexus: **"Procedure
  'dbms.components' not found"**. Same family as §3 (db.*
  procedures) but different namespace and a cleaner
  failure mode (explicit "not found" instead of yielding 0).
- `subquery.count_subquery` — Nexus parses `COUNT { }` but
  returns **null** for the per-row count where Neo4j returns
  the actual degree. Same family as §5 (subquery expression
  drop).
- `write.unwind_create_batch` — Nexus's `UNWIND range(1,10)
  AS i CREATE (:X) RETURN count(*)` returns **1** where Neo4j
  returns **10**. UNWIND is producing a single aggregated row
  before CREATE+count see it; similar shape to §9 (stdev not
  aggregating across rows).

Snapshot: `docs/benchmarks/baselines/2026-04-20-run6.{md,json}`.

### Run 5 — 2026-04-20 · Nexus commit `bae6bebb` · 42 catalog / 40 ran

Catalogue grew to 42 after `aggregation.stdev_score`,
`filter.label_and_id`, `scalar.unwind_range_count`,
`scalar.list_reverse` landed. Three of the four content-match
Neo4j on the first run; `stdev()` surfaces a new aggregation
bug.

| Bucket | Count |
|---|---|
| ⭐ Lead | 34 |
| ✅ Parity | 4 |
| ⚠️ Behind | 0 |
| 🚨 Gap | 2 |
| content-divergent | 11 |
| bench-aborting errors | 4 (now including `aggregation.stdev_score`) |

New finding vs Run 4:

- `aggregation.stdev_score` — `MATCH (n:A) RETURN stdev(n.score)
  AS sd` returns 20 rows on Nexus (one per matched :A node),
  not one aggregated row. Cypher aggregation contract says
  unique-per-group, one row per distinct group; with no GROUP BY
  that's one row total. Nexus's `stdev()` behaves like a
  scalar pass-through. Filed as §9 below. Very likely the
  same root cause as "the rest of the statistical
  aggregations aren't wired up" — `percentileCont`, `variance`,
  `collect` (the latter seems to work but only just).

Snapshot: `docs/benchmarks/baselines/2026-04-20-run5.{md,json}`.

### Run 4 — 2026-04-20 · Nexus commit `4b8ece39` · 38 catalog / 37 ran

Catalogue grew to 38 after `scalar.string_concat`,
`scalar.list_indexing`, `scalar.case_simple`,
`write.create_delete_cycle` landed. The three scalars all pass
clean; the write-cycle scenario exposes a new Nexus restriction.

| Bucket | Count |
|---|---|
| ⭐ Lead | 31 |
| ✅ Parity | 4 |
| ⚠️ Behind | 0 |
| 🚨 Gap | 2 |
| content-divergent | 11 (same set as Run 3, one row fewer because `subquery.with_filter_count` hit expected-rows error this run instead of content mismatch — same bug) |
| bench-aborting errors | 3 (`procedure.db_indexes`, `subquery.with_filter_count`, `write.create_delete_cycle`) |

New finding vs Run 3:

- `write.create_delete_cycle` — Nexus rejects
  `CREATE (n:BenchCycle) WITH n DELETE n RETURN 'done' AS status`
  with the parse-time error **"DELETE requires MATCH clause"**.
  Neo4j accepts the same statement. The WITH binding from the
  CREATE should satisfy DELETE's node-context requirement, but
  Nexus only accepts DELETE when a MATCH is the direct upstream.
  New §8 below.

Snapshot: `docs/benchmarks/baselines/2026-04-20-run4.{md,json}`.

### Run 3 — 2026-04-20 · Nexus commit `6a9983f4` · 34 scenarios

Catalogue grew to 34 after `procedure.db_indexes`,
`subquery.unwind_sum`, `subquery.with_filter_count`,
`subquery.size_of_collect` landed — spreading the surface
across procedure / UNWIND / WITH-pipeline / list-function
territory.

| Bucket | Count |
|---|---|
| ⭐ Lead | 26 |
| ✅ Parity | 4 |
| ⚠️ Behind | 2 |
| 🚨 Gap | 2 |
| content-divergent | **13** |
| bench-aborting errors | **1** |

New findings vs Run 2:

- `procedure.db_indexes` — Nexus's parser rejects
  `CALL db.indexes() YIELD *` (column 25). Add `YIELD *`
  support or rewrite the scenario with explicit yield
  columns. Also independently the procedure is likely
  broken the same way `db.labels()` is — §3 territory.
- `subquery.with_filter_count` — `MATCH (n:A) WITH n.score AS s
  WHERE s > 0.1 RETURN count(*) AS c` returns zero rows on
  Nexus. Neo4j returns 1. Probably the same root as §5
  (WITH → RETURN pipeline collapses) but with a WHERE in
  between; if §5 fix covers it, this scenario turns green
  automatically.
- `subquery.size_of_collect` — `MATCH (n:A) WITH
  collect(n.id) AS ids RETURN size(ids) AS s` returns the
  raw list `[0..19]` instead of 20. `size()` call is not
  being evaluated; the WITH projection leaks straight
  through. Maps to §5.
- `order.bottom_5_by_score` — the same null-positioning bug
  from §7, but in the ASC direction: Nexus puts null first
  in ASC, openCypher says null goes last. One fix covers
  both directions.

Snapshot: `docs/benchmarks/baselines/2026-04-20-run3.{md,json}`.

### Run 2 — 2026-04-20 · Nexus commit `6a9983f4` · 32 scenarios

Catalogue grew to 32 after `traversal.cartesian_a_b`,
`write.create_singleton` (literal mark, not id), `write.merge_singleton`,
`write.set_property` landed. Shortest-path scenario (§3.5)
pulled temporarily — Nexus parser does not accept
`shortestPath((…)-[*]->(…))` yet.

| Bucket | Count |
|---|---|
| ⭐ Lead | 26 |
| ✅ Parity | 4 |
| ⚠️ Behind | 0 |
| 🚨 Gap | **2** |
| content-divergent | **10** |

New divergences vs Run 1:

- `aggregation.avg_score_a`: float-accumulation order drift
  — Nexus `0.09499999999999999`, Neo4j `0.09500000000000003`.
  Both are wrong to different ULPs. Classified as §6 below
  (likely non-blocking, but documented).
- `order.top_5_by_score`: with SmallDataset + TinyDataset
  both loaded, SmallDataset nodes have a null `score`. Neo4j's
  `ORDER BY n.score DESC` puts nulls first in DESC, so the
  top 5 are null-named SmallDataset rows. Nexus returns
  TinyDataset's `n0` instead — Nexus's null ordering differs.
  Classified as §7 below.

Gaps (performance, not correctness):

- `traversal.cartesian_a_b`: Nexus **735 957 µs** vs Neo4j 2 559 µs
  (**287× slower**). Cartesian (`MATCH (a:A), (b:B) RETURN count(*)`)
  builds a 400-row intermediate on both engines but Nexus's
  materialisation is dramatically slower. Not a correctness
  bug; worth its own task if the comparison ever becomes a
  priority.
- `write.set_property`: 4 678 µs vs 2 174 µs (2.15×, just past
  the Gap threshold). Within noise of ⚠️ Behind.

### Run 1 — 2026-04-20 · Nexus commit `04b874c8` · 28 scenarios

First real comparative run.

| Bucket | Count |
|---|---|
| ⭐ Lead | 23 |
| ✅ Parity | 3 |
| ⚠️ Behind | 2 |
| 🚨 Gap | 0 |
| content-divergent | **8** |

Snapshot: `docs/benchmarks/baselines/2026-04-20-baseline.{md,json}`.

The eight originally-surfaced bugs sit below. Every entry names
the exact scenario that caught it so the re-run after each fix
points at the row that just turned green.

## What Changes

Each bug lives in `crates/nexus-core`. The bench keeps driving
them — no bench-side code changes are needed to land the fixes.

### 1. Composite `:Label {prop: value}` filter ignored (HIGH)

Caught by:
- `traversal.small_one_hop_hub`: Nexus=**99**, Neo4j=5
- `traversal.small_two_hop_from_hub`: Nexus=**97**, Neo4j=5

Reproducer (against a database with TinyDataset + SmallDataset
both loaded):

```cypher
MATCH (:P {id: 0})-[:KNOWS]->(b) RETURN count(b) AS c
-- expected: 5 (the five neighbours of SmallDataset's p0)
-- observed: 99 (looks like every KNOWS edge whose tail is any node,
--               i.e. `:P` + `{id: 0}` together got dropped)
```

Label-only (`MATCH (:P)...`) and property-only
(`MATCH (x {id: 0})...`) work; the interaction of the two in a
single pattern element is the broken layer.

### 2. Variable-length path `*m..n` returns empty (HIGH)

Caught by:
- `traversal.small_var_length_1_to_3`: Nexus=**0**, Neo4j=15

Reproducer:

```cypher
MATCH (:P {id: 0})-[:KNOWS*1..3]->(n) RETURN count(DISTINCT n) AS c
-- expected: 15 (5 at 1-hop + 5 at 2-hop + 5 at 3-hop from the hub)
-- observed: 0 (variable-length expansion returns empty set)
```

Partly downstream of §1 (the anchor filter is the same shape),
but `MATCH (a)-[:KNOWS*1..3]->(n)` without the label scope is
worth a separate reproducer to isolate.

### 3. `db.*` procedures return 0 counts (MEDIUM — three scenarios)

Caught by:
- `procedure.db_labels`: Nexus=**0**, Neo4j=6
- `procedure.db_relationship_types`: Nexus=**0**, Neo4j=1
- `procedure.db_property_keys`: Nexus=**0**, Neo4j=205

Reproducer:

```cypher
CALL db.labels() YIELD label RETURN count(label) AS c
-- expected: 6 on the merged fixture (A,B,C,D,E,P)
-- observed: 0
```

The procedure runs, returns a single row with `count(label) =
0`, but the catalog actually has six labels. Either
`db.labels()` yields an empty stream, or `YIELD label` is not
passing rows downstream.

### 4. Integer arithmetic returns a float (LOW)

Caught by:
- `scalar.arithmetic`: Nexus=**7.0**, Neo4j=7

```cypher
RETURN 1 + 2 * 3 AS n
-- expected: integer 7
-- observed: floating-point 7.0
```

Nexus promotes integer-only expressions to `f64`. Affects
every integer result that happens to cross the expression
evaluator. Real-world impact modest; test fixtures that
compare against `json!(7)` trip.

### 5. `WITH` → `RETURN <expr>` leaks WITH payload (MEDIUM — three scenarios)

The post-WITH RETURN clause is being dropped — every scenario
whose RETURN applies a function or boolean expression to
WITH-projected variables gets the raw WITH payload back
instead of the expression's value.

Caught by:

- `subquery.exists_high_score`: Nexus=**[0.99, 150]**,
  Neo4j=false
  ```cypher
  MATCH (n) WITH count(n) AS total, max(n.score) AS hi
  RETURN hi > 0.99 AS any_high
  -- expected: Bool(false) (0.99 is not strictly > 0.99)
  -- observed: [hi, total] raw instead of evaluating
  --           `hi > 0.99`
  ```
- `subquery.size_of_collect`: Nexus=**[0..19 list]**, Neo4j=20
  ```cypher
  MATCH (n:A) WITH collect(n.id) AS ids RETURN size(ids) AS s
  -- expected: Number(20)
  -- observed: the collect'ed list leaks through unchanged;
  --           `size(ids)` is not evaluated
  ```
- `subquery.with_filter_count`: Nexus=**0 rows**, Neo4j=1 row
  ```cypher
  MATCH (n:A) WITH n.score AS s WHERE s > 0.1 RETURN count(*) AS c
  -- expected: 1 row (count aggregation always produces one)
  -- observed: zero rows — the aggregation after the WHERE
  --           returned nothing, or the whole pipeline
  --           short-circuited
  ```

All three strongly suggest the RETURN clause after a WITH is
either not being planned or its expression is being silently
replaced with the WITH column projection.

### 6. Float-accumulation order in `avg()` (LOW — diagnostic)

Surfaced in Run 2 by `aggregation.avg_score_a`:

```cypher
MATCH (n:A) RETURN avg(n.score) AS s
-- Nexus: 0.09499999999999999
-- Neo4j: 0.09500000000000003
```

Both engines are within 4 ULPs of 0.095, so neither is
"correct" in IEEE-754 terms — the difference is summation
order. Classifying as LOW because the user-facing impact is
minor (no sane test asserts on the 15th decimal). Worth
documenting so nobody chases it as a "bug" — it's an
engineering tradeoff (Nexus likely sums naively, Neo4j may
use Kahan compensation or a different block order).

Fix direction: pick one of
(a) Nexus switches to Kahan summation in `sum()` / `avg()`
(b) the divergence guard tolerates a per-ULP epsilon on
floats
(c) leave as-is and declare the divergence informational.

### 7. `ORDER BY` null-positioning inverted in both directions (MEDIUM — two scenarios)

Caught by both `order.top_5_by_score` (DESC, Run 2) and
`order.bottom_5_by_score` (ASC, Run 3):

```cypher
-- DESC (openCypher: nulls first)
MATCH (n) RETURN n.name ORDER BY n.score DESC LIMIT 5
-- Nexus: "n0"   (nulls sorted LAST — wrong)
-- Neo4j: null   (nulls sorted FIRST — correct)

-- ASC (openCypher: nulls last)
MATCH (n) RETURN n.name ORDER BY n.score ASC LIMIT 5
-- Nexus: null   (nulls sorted FIRST — wrong)
-- Neo4j: "n0"   (nulls sorted LAST — correct)
```

Nexus sorts nulls with the wrong polarity in **both** DESC
and ASC. openCypher: DESC → nulls first, ASC → nulls last.
Medium severity because `ORDER BY` is everywhere; fix is a
single polarity flip in the comparator.

### 8. `DELETE` rejects CREATE→WITH-flow context (MEDIUM)

Caught by:
- `write.create_delete_cycle` (Run 4): Nexus parse-time error
  **"DELETE requires MATCH clause"**

```cypher
CREATE (n:BenchCycle) WITH n DELETE n RETURN 'done' AS status
-- Neo4j: executes the statement; 1 node created + deleted; returns 'done'
-- Nexus: parse error, "DELETE requires MATCH clause"
```

The DELETE clause should accept any node binding already in
scope — whether that came from `MATCH`, `CREATE`, or a prior
`WITH`. Nexus's current check insists the node variable comes
from a MATCH. Blocks iteration-safe create-then-delete
patterns in the bench, and — more importantly — any real
transactional flow that creates temp data and immediately
cleans it up.

### 9. Statistical aggregations don't aggregate (MEDIUM)

Caught by:
- `aggregation.stdev_score` (Run 5): Nexus returns **20 rows**,
  Neo4j returns **1** (the actual stdev value).

```cypher
MATCH (n:A) RETURN stdev(n.score) AS sd
-- Neo4j: one row, stdev value ≈ 0.0577
-- Nexus: 20 rows, one per matched :A node; stdev() appears to
--        pass its argument through unchanged instead of
--        aggregating across the row set
```

`stdev()` is not being recognised as an aggregation function
in the planner — otherwise its presence alone would collapse
the row set to one. Almost certainly extends to `stdevp`,
`variance`, `percentileCont`, `percentileDisc` the same way.
Fix direction: declare these in the aggregation-function
registry the planner consults when deciding "should this
expression collapse rows".

### Methodology

For every bug:

1. Land a failing regression test at the executor layer
   (`crates/nexus-core/src/engine/tests.rs`) — mirrors the
   pattern set by `phase6_nexus-delete-executor-bug` and
   `phase6_nexus-create-bound-var-duplication`, so the fix is
   locked in without needing a live server.
2. Fix the narrowest layer (parser / planner / executor
   operator / procedure handler / expression evaluator).
3. Rebuild `target/release/nexus-server.exe`, rerun
   `nexus-bench --compare`, update the bench table in this
   proposal's reference section with the new numbers.
4. Tick the corresponding item in `tasks.md`.

## Impact

- Affected code:
  - `crates/nexus-core/src/executor/**` — filter composition,
    variable-length path, expression evaluator.
  - `crates/nexus-core/src/executor/procedures/**` (or
    wherever `db.labels()` lives) — procedure yield wiring.
  - `crates/nexus-core/src/engine/tests.rs` — regression tests.
- Affected bench fixtures: none — the scenarios are already
  shaped correctly; they just expose Nexus bugs.
- Breaking change: NO. Every fix restores Neo4j-compatible
  behaviour that the 300-test diff suite presumably already
  expected; if any test in that suite regresses it means the
  suite was papering over the same bug.
- User benefit: the bench's `Ratio` column stops comparing
  wrong answers, the Nexus-vs-Neo4j compatibility claim gets
  an independent verification beyond the diff suite, and four
  real user-facing correctness gaps (label+prop filter,
  variable-length paths, catalog procedures, WITH→RETURN
  projection) close.

Source: first real `nexus-bench --compare` run,
`target/bench/report.md` at commit `04b874c8`.
