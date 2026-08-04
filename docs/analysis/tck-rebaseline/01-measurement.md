# 01 — Measurement: the 48.8% re-baseline

## F-100 — What moved between 13.2% and 48.8%

`docs/analysis/tck/07-execution-plan.md` projected `~55–68%` after its Phase 4 and
`~72–85%` after temporal. The archive shows Phases 0–4 and the whole temporal chain
landed (43 archived `phase21_tck-*` tasks, `2026-07-27` → `2026-08-04`), and the
measured result is **48.8%**. The shortfall is not a failure of those tasks — each
fixed what it targeted. It is that the estimates were built on category percentages,
and **category percentages hide single defects with 400-scenario blast radius**.

The re-baseline replaces estimates with counts. Source of truth:
`crates/nexus-core/target/tck-failures-orderby1.jsonl` — 1908 rows, one per failing
scenario, each carrying category, feature file, scenario name, query, panic message,
and the last engine error with its `OpenCypherErrorKind`. Row count matches the
committed report's fail count exactly, so the log *is* the report.

**Confidence.** High.

---

## F-101 — Failure-family taxonomy (mechanical, over the whole log)

Grouping the 1908 by the *shape* of the assertion that failed, before any
root-causing:

| Family | Fails | % | Meaning |
|---|---:|---:|---|
| `row-count mismatch` | 614 | 32.2% | the engine returned the wrong number of rows |
| value mismatch (`expected row … not found`) | 450 | 23.6% | right shape, wrong value |
| missing error (`expected a X but the query succeeded`) | 289 | 15.1% | negative test the engine accepts |
| error-detail token absent | 126 | 6.6% | error raised, message lacks the required token |
| column-name mismatch | 111 | 5.8% | projection named the column wrong |
| parser rejected a valid query | 105 | 5.5% | grammar gap |
| runtime error on a valid query | 86 | 4.5% | executor refuses something legal |
| column-**count** mismatch | 57 | 3.0% | projection produced the wrong arity |
| side-effect counters | 27 | 1.4% | counters diverge from spec |
| error-**kind** misclassified | 25 | 1.3% | right error, wrong `OpenCypherErrorKind` |
| row order / row content | 12 | 0.6% | ordering |
| other | 6 | 0.3% | — |

The single most useful reading: **`row-count mismatch` at 32% is not 614 problems.**
479 of them are RC01, one defect. The taxonomy is a triage tool, not a work breakdown.

**Confidence.** High (mechanical).

---

## F-102 — Attributed root-cause table (sums to 1908)

Each failure assigned to exactly one root cause, by rule, over the whole log. The
classifier is deterministic and re-runnable; the rules are stated in each finding.

| RC | Root cause | Fails | Dominant categories |
|---|---|---:|---|
| RC01 | quantifier in a standalone projection → 0 rows | 479 | quantifier 479 |
| RC03 | temporal construction from another temporal | 176 | temporal 176 |
| RC11 | missing runtime type guards / error-kind | 103 | list 44, typeConversion 22, graph 15 |
| RC04 | sub-second component composition | 102 | temporal 102 |
| RC20 | misc negative-test validation | 96 | literals 24, list 15, quantifier 12 |
| RC06 | variable-reuse / type-conflict validation | 96 | match 91 |
| RC13 | SKIP / LIMIT semantics | 89 | with-orderBy 58, return-skip-limit 12 |
| RC19 | parser: literals, identifiers, slices | 78 | precedence 18, list 16, match 7 |
| RC10 | restricted write pipeline | 67 | set 26, remove 24, merge 13 |
| RC07 | `p = pattern` not allowed after a comma | 66 | match 64 |
| RC02 | aggregation grouping key dropped | 64 | quantifier 64 |
| RC22 | OPTIONAL MATCH / var-length binding identity | 57 | match 13, graph 10, delete 6 |
| RC16 | cross-type + instant ordering | 49 | with-orderBy 33, return-orderby 12 |
| RC05 | week / quarter / ordinal date components | 47 | temporal 47 |
| RC12 | ORDER BY scope + aggregate validation | 46 | with-orderBy 43 |
| RC18 | column-name fidelity (residue after RC14) | 47 | boolean 11, with-orderBy 10 |
| RC08 | positive pattern predicate in WHERE | 34 | pattern 31 |
| RC21 | side-effect counting | 28 | merge 12, delete 11 |
| RC15 | three-valued list/nested comparison | 24 | comparison 13, list 8 |
| RC09 | named path variable unbound | 14 | match 13 |
| RC23 | temporal tail | 8 | temporal 8 |
| **total** | | **1908** | |

RC14 (projection-list truncation) and RC17 (chained-`WITH` null) do not appear as rows
because they *co-occur* with RC18/RC19/RC13 in the log: RC14's failures are counted
under `column-count mismatch` and `column-name mismatch`, RC17's only TCK footprint is
inside RC13's with-orderBy rows. Both are named as their own findings and own tasks
because they are distinct defects with distinct fixes — and because RC14 must be fixed
*before* RC18 can be measured honestly.

**Confidence.** High on the totals. Medium on RC-per-row attribution at the margins
(a scenario can be gated by two causes; the rule picks the *first* blocker).

---

## F-103 — Outline inflation is still the dominant distortion

27 distinct temporal scenarios produce 334 failures. 23 distinct `Match6` scenarios
produce 95. Four `Quantifier*.feature` scenario outlines produce 404 of the 479 RC01
rows. **Scenario count still measures coverage, not effort** — the correction the
first analysis made (F-001) holds, and this re-baseline is the proof: 1908 failures,
23 causes.

Practical consequence for planning: **rank tasks by attributed fails ÷ effort band,
never by category percentage.** `expressions/quantifier` at 8.1% is an `S`;
`clauses/match` at 41.2% contains the `L` (RC22).

**Confidence.** High.

---

## F-104 — What the report's 71 skips are

50 are the deliberate procedure-registration skip-list (`clauses/call`). The other 21
are scenarios that hit a Gherkin step the runner does not define, spread across
`map` (9), `graph` (3), `merge` (3), and singles elsewhere. They are **not** in the
1908 and are not addressed by any task here; closing them is harness work with no
conformance-percentage cost until the underlying features pass anyway.

**Confidence.** High.

---

## F-105 — Regression guard: the two-number story must hold

Every task in this plan touches the evaluator, the parser, the planner, or the write
path. The hard constraint from the first analysis (`../tck/07-execution-plan.md`
F-045) is unchanged and now has a second leg:

1. **Neo4j differential suite must stay at 300/300** —
   `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`.
2. **Workspace suite must stay green with `--no-fail-fast`** — a plain
   `cargo test --workspace` aborts at the first failing target and silently skips
   ~3000 tests.
3. **TCK must be judged per category against an identical re-run** — the corpus has
   category-local nondeterminism (±4 on `Merge5[4]`); a flat global band hides real
   regressions.

Tasks that add strictness (RC11, RC06, RC12, RC20) are the highest regression risk:
they turn silent success into errors, and internal tests may lean on the leniency.

**Confidence.** High.
