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

## Bench runs (append-only log; one entry per re-run)

Snapshots live under
`docs/benchmarks/baselines/`. Every row references the exact
Nexus commit the bench ran against so a latency regression
(or a fix) has a commit to point at.

### Run 2 — 2026-04-20 · Nexus commit pending · 32 scenarios

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

### 5. `WITH` → `RETURN bool` returns the WITH payload (MEDIUM)

Caught by:
- `subquery.exists_high_score`: Nexus=**[0.99, 150]**,
  Neo4j=false

```cypher
MATCH (n) WITH count(n) AS total, max(n.score) AS hi
RETURN hi > 0.99 AS any_high
-- expected: boolean false (0.99 is not strictly > 0.99)
-- observed: a row with the WITH projection [hi, total] instead
--           of the RETURN expression applied to it
```

Either the planner drops the computed-RETURN clause after a
WITH, or the RETURN expression is not being evaluated and the
WITH bindings leak out raw.

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

### 7. `ORDER BY <prop> DESC` puts nulls last, not first (MEDIUM)

Surfaced in Run 2 by `order.top_5_by_score`:

```cypher
MATCH (n) RETURN n.name AS name ORDER BY n.score DESC LIMIT 5
-- With TinyDataset + SmallDataset both loaded, SmallDataset
-- rows have null score.
-- Nexus top row: "n0" (a TinyDataset name — nulls sorted LAST)
-- Neo4j top row: null (SmallDataset rows sorted FIRST in DESC)
```

openCypher semantics: in DESC, nulls come **first**; in ASC,
nulls come **last**. Neo4j follows this; Nexus does not.
Medium severity because `ORDER BY` is everywhere and
null-sorting surprises trip real queries.

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
