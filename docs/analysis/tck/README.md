# Road to 100% openCypher TCK Conformance

> Planning analysis for the release **after 3.0.0**. Grounded in the committed
> baseline `docs/compatibility/OPENCYPHER_TCK_REPORT.md` (509/3868 = 13.2%),
> a live re-run of the harness, and a six-way source audit of the parser,
> planner, executor, evaluator, write path, temporal subsystem, and the TCK
> harness itself. Every finding is anchored to `file:line` or a named `.feature`
> scenario.

## Executive summary

Nexus passes **509 of 3868** vendored openCypher TCK scenarios (**13.2%**). That
number is honest but easy to misread. Three framing facts reshape the whole plan:

1. **Scenario count ≠ work.** 276 `Scenario Outline`s expand into the 3868 total
   (temporal: 72 outlines → 1004 scenarios; quantifier: 70 → 604; with-orderBy:
   57 → 292). A *single* missing feature fails hundreds of scenarios. The
   conformance % is a valid **coverage** metric but a poor **effort** estimate —
   the real work is a few dozen root-cause clusters, not 3175 independent bugs
   (F-001).

2. **"Category" = the directory of the `.feature` file, not the feature under
   test.** `clauses/match` at 3% is *not* "matching is broken" — a ground-truth
   harness run shows ~71% of its failures are **negative-test scenarios** that
   expect an error Nexus never raises. `clauses/with-orderBy` at 9% is
   **temporal-gated**, not ordering-gated. Reading the directory name as the
   feature health is the single most common way to mis-plan this work (F-002).

3. **The ROADMAP-vs-TCK gap is expected, not a contradiction.** `docs/ROADMAP.md`
   marks temporal, variable-length paths, and the write clauses "COMPLETE" on the
   strength of **210/210 Neo4j differential** and **300/300 diff-suite** passes.
   Those are *curated agreement with one implementation on a happy path*. The TCK
   is the *strict spec with no partial credit* and exercises the full semantic
   surface — nulls, error kinds, comparability, timezones, canonical rendering.
   The work to 100% TCK is overwhelmingly **deepening semantics**, not building
   features from scratch (F-003).

### Where the gap actually lives

The 3175 failures collapse onto **six cross-cutting workstreams** plus **two large
self-contained subsystems**. Most of the leverage is *not* in net-new engine code:

| Lever | Nature | Rough scenarios gated¹ | Band |
|---|---|---:|---|
| **W-A — Error semantics** (semantic-validation pass + runtime type-guards + `OpenCypherErrorKind` + detail-token emission) | The dominant architectural gap | 600–900+ | L→XL |
| **W-B — Three-valued (Kleene) logic** (stop coercing null/wrong-type; propagate) | Shared eval-helper fix | 200–300 | M |
| **W-C — Result-shape & column-name & side-effect fidelity** | Mechanical correctness | 150–250 | S→M |
| **W-D — Typed value model** (temporal; later non-finite floats) | Self-contained subsystem | ~935 (temporal) | XL |
| **W-E — Parser breadth** (bracket-less rels, quantifier `IN…WHERE`, XOR/`^`/`%`/chaining, literals, `SET =`) | Grammar gaps | 700+ | M |
| **W-F — Harness & measurement** (Step-0 failure log, parameters, control-query, fixtures) | Test infrastructure | ~95 skips + de-noise | XS→S |

¹ Estimates from scenario counts; they **overlap heavily** (a temporal scenario in
`with-orderBy` also needs column-naming and 3VL right) and therefore do **not**
sum. See `07-execution-plan.md` for the dependency-aware, non-double-counted plan.

### The one thing to do first

**The harness discards the reason for every failure** — the `after` hook counts a
`StepFailed` and throws away the panic payload (`tck_opencypher.rs:293-303`). Until
that is fixed, every effort estimate here is an educated guess. **Step 0 is an
`XS` (~half-day) change** to emit a per-scenario JSONL failure log (category,
scenario, query, expected-vs-got). It converts 3175 opaque fails into a handful of
buckets and lets the team measure — rather than estimate — how much is harness
false-fail vs real engine gap (F-004, F-014).

### Is 100% realistic?

**As a literal target, no — and that is the honest headline.** The openCypher TCK
encodes behaviours that even Neo4j (the reference implementation) diverges on,
plus scenarios gated on capabilities with near-zero product value here (ad-hoc
procedure registration, historical-timezone offsets, non-finite float values). A
disciplined push through W-A…W-F plus the temporal subsystem plausibly moves
Nexus from **13% to the 70–85% band**; the last stretch is a long tail of
disputed or low-value scenarios. This analysis therefore optimises for
**conformance-per-effort**, sequences the high-yield levers first, and names the
tail explicitly rather than pretending 100% is a clean finish line
(`07-execution-plan.md`).

## Index

| File | Theme |
|---|---|
| [01-measurement-and-methodology.md](01-measurement-and-methodology.md) | What 13.2% means; category≠feature; outline inflation; harness false-fails; the ROADMAP-vs-TCK reconciliation; Step-0 instrumentation. **F-001–F-008** |
| [02-cross-cutting-workstreams.md](02-cross-cutting-workstreams.md) | The six-workstream spine (W-A…W-F) that most failures share. **F-009–F-014** |
| [03-temporal.md](03-temporal.md) | The XL subsystem: no typed temporal value; duration-as-object wall; ISO-8601; accessors; truncation; timezone DB. **F-015–F-020** |
| [04-expressions.md](04-expressions.md) | The boolean-4% root cause; the coercion theme; parser literals/precedence; functions; typeConversion; aggregation (already built). **F-021–F-028** |
| [05-quantifier-patterns-paths.md](05-quantifier-patterns-paths.md) | The big reclassification: `quantifier` = list predicates, not QPP (which is already built); pattern predicates/EXISTS/comprehension; path values. **F-029–F-034** |
| [06-clauses-read-write.md](06-clauses-read-write.md) | match (negative-heavy), with-orderBy (temporal-gated), ORDER BY expressions, the write clauses (DELETE one-bug, SET, counting), CALL deprioritised. **F-035–F-042** |
| [07-execution-plan.md](07-execution-plan.md) | Phased, dependency-aware roadmap: effort bands, sequencing, expected recovery per phase, and the honest expectation on 100%. **F-043–F-048** |

## Method & confidence

- Six parallel source-audit agents, one per cluster, each cross-referencing the
  actual `.feature` corpus against the engine. The read-clause agent additionally
  **ran the harness live** (`NEXUS_TCK=1`) and bucketed real panic signatures, so
  the `clauses/*` breakdown in `06-*` is measured, not inferred.
- Confidence is stated per finding. The **root-cause direction** is high
  confidence throughout; **absolute scenario recovery** per lever is medium
  confidence until the Step-0 log (F-004) replaces estimates with counts.

## Reproduce the baseline

```
NEXUS_TCK=1 cargo +nightly test -p nexus-core --test tck_opencypher --all-features
# or
scripts/compatibility/run-opencypher-tck.ps1
```
