# 06 — Execution plan

23 tasks, four phases, sequenced by **attributed fails ÷ effort** with the dependency
edges respected. Bands: **XS** ≈ hours · **S** ≈ days · **M** ≈ 1–2 weeks · **L** ≈ weeks.

Unlike `../tck/07-execution-plan.md`, the counts here are **measured**, not estimated —
but they are still **upper bounds**, because a scenario gated by two causes flips only
when both land. Overlaps are called out per row.

---

## F-160 — Sequencing principle: stop the silent failures, then take the 479

Two changes from the old plan's ordering:

1. **The silent-failure defects go first, ahead of everything.** RC17 (chained `WITH`
   → null) and RC14 (projection list truncated without an error) make the engine return
   confidently wrong answers. RC14 additionally contaminates the measurement of RC18
   and RC19, so fixing it first makes two later tasks sizeable.
2. **The single 479-scenario defect goes second.** RC01 is an `S` that moves the
   headline number more than the entire temporal subsystem did. It was invisible to the
   old plan because `expressions/quantifier` at 8.1% read as a missing feature.

**Confidence.** High.

---

## Phase A — hazards and mega-yield · *`S`–`M`, mostly parallel*

| # | Task | RC | Fails | Band | Notes |
|---|---|---|---:|---|---|
| A1 | `phase22_tck-chained-with-alias-null` | RC17 | ~0 | S | silent data loss; first on severity, not count |
| A2 | `phase22_tck-quantifier-standalone-projection` | RC01 | 479 | S | confirm the mechanism before editing |
| A3 | `phase22_tck-projection-list-silent-truncation` | RC14 | ~90 | M | **blocks** D4, D5 |
| A4 | `phase22_tck-aggregation-grouping-key` | RC02 | 64 | S | also fixes F-140; interacts with B3 |
| A5 | `phase22_tck-temporal-subsecond-components` | RC04 | 102 | S | cheapest 100 in the corpus |

A2 and A5 are fully independent of everything else and of each other. A1/A3/A4 all sit
in the projection path — serialise those three.

**Exit criterion:** re-run the harness, commit the regenerated report, and confirm
`expressions/quantifier` has moved from 8.1% into the high 80s. If it has not, A2's
mechanism was misidentified — stop and re-root-cause rather than proceeding.

---

## Phase B — parser entry points and small semantics · *`S`, parallel*

| # | Task | RC | Fails | Band | Notes |
|---|---|---|---:|---|---|
| B1 | `phase22_tck-parse-path-assignment-in-pattern-list` | RC07 | 66 | S | **blocks** C4; its own 66 flip only with C4 |
| B2 | `phase22_tck-parse-positive-pattern-predicate` | RC08 | 34 | S | evaluation already works via the `NOT` path |
| B3 | `phase22_tck-skip-limit-semantics` | RC13 | 89 | S | re-measure after A4 |
| B4 | `phase22_tck-side-effect-counting-completion` | RC21 | 28 | S | exact assertions, quick |
| B5 | `phase22_tck-three-valued-comparison-lists` | RC15 | 24 | S | extends landed top-level 3VL |

All five are independent. This is the most parallelisable phase in the plan.

---

## Phase C — semantics deepening · *`M`*

| # | Task | RC | Fails | Band | Notes |
|---|---|---|---:|---|---|
| C1 | `phase22_tck-temporal-select-from-temporal` | RC03 | 176 | M | all of `Temporal3.feature` |
| C2 | `phase22_tck-temporal-week-quarter-ordinal` | RC05 | 47 | M | ISO **week-based year**, not calendar year |
| C3 | `phase22_tck-runtime-type-guards` | RC11 | 103 | M | **blocks** D3; highest regression risk |
| C4 | `phase22_tck-validate-variable-reuse` | RC06 | 96 | M | **after B1**; +B1's 66 land here |
| C5 | `phase22_tck-validate-orderby-scope` | RC12 | 46 | S | shares the scope helper with C4 |
| C6 | `phase22_tck-ordering-cross-type-and-instant` | RC16 | 49 | M | one comparator for ORDER BY, min, max |
| C7 | `phase22_tck-named-path-value` | RC09 | 14 | M | source-confirmed; unblocks `MERGE p =` in D1 |

C1/C2 (temporal) and C3 (guards) and C6 (comparator) are independent tracks. C4 needs
B1; C5 should follow C4 to reuse the scope helper.

---

## Phase D — the architectural items and the residue · *`M`–`L`*

| # | Task | RC | Fails | Band | Notes |
|---|---|---|---:|---|---|
| D1 | `phase22_tck-write-path-general-pipeline` | RC10 | 67 | L | **expects sub-decomposition** (5 steps, F-150) |
| D2 | `phase22_tck-optional-match-and-varlength-bindings` | RC22 | 57 | L | **expects sub-decomposition**; semantic, not surface |
| D3 | `phase22_tck-validate-negative-tests` | RC20 | 96 | M | **after C3** (needs the error-kind table) |
| D4 | `phase22_tck-parse-literals-and-identifiers` | RC19 | 78 | M | **after A3**; re-measure first, count will shrink |
| D5 | `phase22_tck-column-name-fidelity-residue` | RC18 | 47 | S | **after A3**; do not size before |
| D6 | `phase22_tck-temporal-tail` | RC23 | 8 | M | includes the statement-clock requirement |

D1 and D2 are the only `L`s in the plan and the only tasks that should be expected to
spawn their own checklists. Both are back-loaded deliberately: D1 benefits from C7
(`MERGE p = …`) and from A3 (the write parser inherits expression parsing), and D2's
OPTIONAL-MATCH null-row half overlaps D1's step 4.

---

## F-161 — Expected trajectory (measured counts, overlap-discounted)

| After | Fails remaining | Pass % | Driver |
|---|---:|---:|---|
| baseline | 1908 | 48.8% | — |
| A | ~1200 | ~69% | RC01 alone is +12pp |
| B | ~1030 | ~73% | five independent `S` items |
| C | ~530 | ~86% | temporal components + validation + guards |
| D | ~230 | ~94% ceiling-limited | write pipeline, bindings, negative tests |

The `D` row is the optimistic bound. Discounting for double-gated scenarios and for
the negative-test tail whose detail tokens the spec under-specifies, the honest
expectation for A–D complete is the **85–92% band** stated in the README. Roughly 210
of the current 1908 are error-taxonomy scenarios and ~70 have no product value here
(procedure registration, ±10⁹-year ranges, historical timezone offsets) — that residue
is the ceiling, and it is the same tail `../tck/07-execution-plan.md` named, now sized.

**Re-baseline after every phase.** Judge each category against an identical re-run of
itself, never against the grand total (`project-tck-noise-is-category-specific`).

**Confidence.** Medium-high. Counts are measured; the overlap discount is judgement.

---

## F-162 — Hard gates on every task

Non-negotiable, and the reason several `S` tasks will feel like `M`:

1. **Neo4j differential suite 300/300** —
   `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`.
2. **Workspace suite green with `--no-fail-fast`** — a plain `cargo test --workspace`
   aborts at the first failing target and silently skips ~3000 tests.
3. **`cargo +nightly fmt --all` + `clippy -D warnings`** before every commit.
4. **A regression test per root cause**, using the *minimal* query from this analysis —
   not the TCK scenario. The minimal queries here are the regression suite.
5. **No file over 1500 lines** and **no task/phase identifiers in code, tests, or
   comments** — the phase tag belongs in the commit message and the task record only.

C3, C4, C5, and D3 add strictness where the engine is currently lenient. They are the
highest regression risk in the plan; run gate 1 and 2 **before** committing, not after.

**Confidence.** High.

---

## F-163 — Wire the TCK into CI as a ratchet once past 70%

The recommendation from `../tck/07-execution-plan.md` F-047 is now actionable: after
Phase B the number should be ~73%, high enough that a ratchet is meaningful. Fail the
build if the pass count drops below the committed baseline. The differential suite
guards happy-path Neo4j agreement; this guards spec depth. Keep both numbers in
`CLAUDE.md` and the README — they measure different things.

**Confidence.** High.

---

## F-165 — Three pre-existing pending tasks, and where they slot in

`.rulebook/tasks/` already held three un-started `phase21_tck-*` records before this
re-baseline. They are **not** superseded — place them as follows:

| Task | Slot | Why there |
|---|---|---|
| `phase21_tck-cross-type-comparison-semantics` **(in flight — implemented and staged by a concurrent session as this was written, +2 scenarios)** | **Phase B**, next to B5 | Same severity class as RC17: `RETURN 1 < 'text'` returns `true` instead of null, so ordinary `WHERE` filters over heterogeneous properties silently keep or drop rows by string spelling. It fixes the **operator** path and explicitly leaves `compare_values_for_sort` alone; C6 fixes the **sort/aggregate** comparator. Run it *before* C6 so the two are not edited blind to each other, and pair it with B5 (RC15) — both are Kleene-propagation work in the same two evaluators. |
| `phase21_tck-non-finite-floats` | **after Phase D** | Value-representation change, low scenario count. Unchanged from `../tck/07-execution-plan.md` Phase 7. |
| `phase21_tck-procedure-registration-call` | **after Phase D** | The 50 deliberate skips. `L` effort, near-zero product value; opportunistic only. |

The one coupling to watch: B5, `phase21_tck-cross-type-comparison-semantics`, and C6 all
touch `eval/predicate.rs` **and** `eval/projection/core.rs`, which implement these
operators separately. A fix in one file leaves the other wrong — verify every change
through a query routed via each path.

**Confidence.** High.

---

## F-164 — Materialisation

All 23 tasks exist as rulebook task records under `.rulebook/tasks/phase22_tck-*`, each
with a `proposal.md` naming its finding and attributed count, and a `tasks.md` whose
checklist order encodes the dependency edges above. Drive them with `/rulebook-driver`
in the phase order given; within a phase, the independent items may run in parallel.

Do **not** re-order across phases. A1–A3 exist to make the later measurements
trustworthy; taking C1 first because temporal looks bigger would repeat exactly the
mis-planning this re-baseline corrects.

**Confidence.** High.
