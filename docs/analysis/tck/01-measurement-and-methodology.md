# 01 — Measurement & Methodology

How to read the 13.2% honestly, why the category table misleads, and the one
instrumentation change that must precede all implementation work.

---

## F-001 — Scenario count overstates distinct defects (outline inflation)

**Evidence.** The corpus is 1339 plain scenarios + **276 `Scenario Outline`s**;
cucumber expands each outline row into its own scenario, producing the 3868 total.
The mega-buckets are outline-dominated: temporal 72 outlines → 1004 scenarios,
quantifier 70 → 604, with-orderBy 57 → 292 (`docs/compatibility/OPENCYPHER_TCK_REPORT.md`;
corpus under `crates/nexus-core/tests/tck/opencypher/features/`).

**Impact.** One unimplemented feature fails hundreds of scenarios. The 3175
failures are **a few dozen root causes**, not 3175 bugs. Any plan that budgets
effort against raw scenario counts will be wildly wrong. Triage and size by
**root-cause cluster**, and treat the conformance % as a coverage tracker only.

**Confidence.** High (counts verified against the report and corpus).

---

## F-002 — "Category" is the `.feature` directory, not the feature under test

**Evidence.** The runner derives the category from the path
(`category_of`, `tck_opencypher.rs:275-291`): `.../features/clauses/match/…` →
`clauses/match`. A live harness run bucketed by real panic signatures shows
`clauses/match` (339 fails) is **~71% negative-test scenarios** (172
"error-expected-but-query-succeeded" + 69 "error-token-missing"), not broken
pattern matching. `clauses/with-orderBy` (264 fails) is **temporal-gated**: ~60
failures are `date()/datetime()/…` constructors dying in the `CREATE` *setup*,
~77 are temporal value rendering in results, ~67 are negative tests — only ~10 are
genuine ordering errors (see `06-clauses-read-write.md`, F-035/F-037).

**Impact.** The per-category pass-% is not feature health. Planning off the
directory name ("ORDER BY is 9% done") is the most common way to mis-scope this
project. Cross-reference every category against the actual `.feature` content and
the failure buckets before assigning work.

**Confidence.** High (measured, not inferred).

---

## F-003 — The ROADMAP-vs-TCK gap is a metric difference, not a regression

**Evidence.** `docs/ROADMAP.md:625-631` marks temporal features "COMPLETED
(2025-11-30)"; line 220 marks "Variable-Length Paths (quantifiers, shortestPath)
COMPLETE"; line 216 marks writes (MERGE/SET/DELETE/REMOVE) COMPLETE — all on the
strength of **210/210 Neo4j compatibility** (`ROADMAP.md:210-212`). The TCK
reports temporal 5.1%, quantifier 2.6%, set 1.9%, delete 0.0%
(`OPENCYPHER_TCK_REPORT.md`). `CLAUDE.md` already states the distinction: "300/300
on the Neo4j differential suite … distinct from openCypher TCK conformance ~13%".

**Impact.** These are **two different measurements**. The Neo4j differential suite
is *curated agreement with one implementation on a happy path*; the TCK is the
*authoritative spec with no partial credit* and drives the full semantic surface
(null handling, error kinds, comparability, canonical rendering, timezones).
Nexus's features are real and pass the happy path — they are **breadth-first, not
depth-complete**. The road to 100% TCK is overwhelmingly *deepening existing
semantics*, which is why so much leverage sits in shared eval/validation layers
(`02-cross-cutting-workstreams.md`) rather than net-new subsystems.

**Confidence.** High.

---

## F-004 — The harness discards every failure reason (Step 0)

**Evidence.** The `after` hook calls `record(category, ev)` and nothing else
(`tck_opencypher.rs:493-497`); `record` maps `StepFailed(..)` to a count slot and
**drops the payload** (`:293-303`). The panic/assertion text (cucumber
`ScenarioFinished::StepFailed`'s error field) — which already renders precise
diagnostics like `"column[0] name mismatch…"`, `"row-count mismatch…"`, `"expected
error kind…"` — is thrown away. The hook already receives `_scenario`
(name + docstrings) and `_world` (`last_error`, `last_error_kind`, `last_result`).

**Impact.** Every scenario recovery estimate in this analysis is currently an
educated guess. The fix is fully self-contained in the `.after(...)` closure at
`tck_opencypher.rs:493`: append a JSONL line per `StepFailed` to
`docs/compatibility/OPENCYPHER_TCK_FAILURES.jsonl` with `{category, feature_path,
scenario_name, query, message, last_error, last_error_kind}`, optionally
auto-bucketed by message prefix (column-name / row-count / value-shape / error-kind
/ error-token). Effort **XS (~half a day, one file)**. This converts 3175 opaque
fails into measurable buckets and is a hard prerequisite for accurate planning.

**Impact ranking: this is the single highest-priority item in the analysis** — not
because it fixes a scenario, but because it makes every subsequent decision
data-driven. **Do it first.**

**Confidence.** High (mechanism confirmed in source).

---

## F-005 — Harness false-fail: un-aliased column names are re-rendered from the AST

**Evidence.** `compare_table` asserts `result.columns[pos] == header` exactly
(`tck_common/mod.rs:45-51`) **before** any row comparison (`:54`), so a name
mismatch masks otherwise-correct rows. For an un-aliased projection item the name
comes from `expression_to_string` (`planner/queries/planner_core.rs:717-721`) and,
for aggregates, from a hard-coded bare name
(`strategy.rs:671` `"count"`, `:700` `"sum"`, `:729` `"avg"`, `:758` `"min"`,
`:787` `"max"`, `:835` `"collect"`). Runtime proof captured:
`column[1] name mismatch: result="count", table="count(*)"` and
`result="max", table="max(n.age)"`. openCypher headers are the **verbatim source
text** (`.../clauses/return/Return8.feature:47` → `count(*)`).

**Impact.** Every un-aliased-aggregation scenario fails on the column name
regardless of value correctness — concentrated in `return`, `return-orderby`,
`aggregation`, `literals`. This is a **harness-visible engine gap** (the engine
*could* name columns verbatim) that overlaps workstream **W-C** (F-011). Secondary
divergences from the same renderer: `<>`→`!=`, `null`→`NULL`, float `1.0`→`1`,
`count(*)`→`count()`/`count(?)`, and odd TCK whitespace (`cOuNt( * )`) that a
re-serialiser cannot reproduce (`planner/queries/expressions.rs:77,79,87,153,184`).

**Confidence.** High (runtime-proven).

---

## F-006 — Harness false-fail: the error detail-token substring gate

**Evidence.** `error_should_be_raised` asserts `msg.contains(token)` for the TCK's
CamelCase detail token unless it is `*` (`tck_opencypher.rs:196-202`). A grep for
those tokens (`UndefinedVariable`, `InvalidArgumentType`, `VariableAlreadyBound`,
…) across `crates/nexus-core/src` returns **zero** matches. So even when Nexus
raises the correct *kind* at the correct phase, the wording gate fails the
scenario. There are **193 error-assertion scenarios** total (163 SyntaxError, 16
TypeError, 5 ArgumentError, rest small; 162 compile-time / 27 runtime / 4 any-time).

**Impact.** Up to 193 scenarios are partially gated on **message wording**, not
error detection. Two clean options: (a) emit the upstream token in the error
message as detection is added (rides on W-A), or (b) relax the harness to
kind-only matching where the TCK token is a well-known synonym. Note the phase
(compile vs runtime) is captured but **not asserted** (`tck_opencypher.rs:176`),
so a *runtime* type-guard is sufficient for many of these — no static checker
required (see F-021).

**Confidence.** High.

---

## F-007 — Skip-list: 164 deliberate skips, several cheap to unlock

**Evidence.** `skip_reason` (`tck_opencypher.rs:313-330`) removes: query
parameters (62), procedure registration (50), named fixture `binary-tree-N` (19),
control-query reference (33). `execute_cypher_with_params` **already exists**
(`engine/query_pipeline.rs:63`); the World only ever calls `execute_cypher` with
no params (`tck_opencypher.rs:92`).

**Impact.** Unlock ranking:
1. **Parameters (62)** — pure plumbing: add a `Given "parameters are:"` step that
   parses the table with the existing `tck_cell_to_json` into a World map and calls
   `execute_cypher_with_params`. ~15 lines, **S**. Many pass immediately.
2. **Control query (33)** — a `When "executing control query:"` alias of
   `executing_query`. **S**. Gated on write correctness (they verify CREATE/MERGE
   state).
3. **Named graph binary-tree (19)** — hard-code the two canonical fixture CREATE
   scripts behind a `Given the binary-tree-N graph` step. **S-M**. Feeds
   `useCases/triadicSelection`; pass depends on triadic MATCH support.
4. **Procedures (50)** — ad-hoc procedure registration + CALL/YIELD. **L**, engine
   work; low product value. Deprioritise (see F-042).

**Confidence.** High.

---

## F-008 — Net attribution: most of the gap is real engine work

**Evidence.** Synthesising F-005–F-007 with the live-run buckets: harness-or-
classification-attributable failures are the un-aliased column-name class (tens,
concentrated in return/aggregation/literals), the error-token gate (up to 193,
overlapping real detection gaps), plus ~95 cheaply-unlockable skips
(parameters 62 + control-query 33). The mega-buckets — temporal 935, quantifier
588, match 339 (mostly missing error detection), with-orderBy 264 (mostly
temporal) — are **overwhelmingly real engine gaps** and will not move via harness
work. A spot-check confirmed quantifier RETURNs are all `AS`-aliased, so those 588
are genuine semantics gaps, not column-name false-fails.

**Impact.** Plan expectation: harness/measurement work (W-F) de-noises the metric
and unlocks ~95 skips cheaply, but **the 70→85% climb is engine semantics** (W-A,
W-B, W-D especially). Do not over-index on the harness fixes as a shortcut.

**Confidence.** Medium-high (exact split pending the Step-0 log, F-004).
