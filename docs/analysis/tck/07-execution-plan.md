# 07 — Execution Plan

A dependency-aware, phased path from 13.2% toward the achievable ceiling. Effort
bands: **XS** ≈ hours · **S** ≈ days · **M** ≈ 1–2 weeks · **L** ≈ weeks · **XL** ≈
1–2 months (one engineer). All scenario-recovery figures are **estimates pending the
Step-0 log (F-004)** and **overlap** (they do not sum) — treat them as direction and
relative magnitude, not commitments.

---

## F-043 — Sequencing principle: measure → low-coupling high-yield → two big bets → tail

The plan front-loads work that is (a) cheap, (b) high scenario-per-effort, and (c)
low-coupling, so the metric climbs fast and de-risks the two large investments
(the static semantic-analysis pass and the temporal subsystem) that follow. The
long tail is scoped explicitly and left last by design.

**Confidence.** High (ordering follows the dependency graph in `02-*`).

---

## Phase 0 — Measurement foundation · **XS→S** · *do first, blocks nothing else's correctness*

| Item | Ref | Band |
|---|---|---|
| Per-scenario failure JSONL log in the `after` hook | F-004 | XS |
| `Given "parameters are:"` step → `execute_cypher_with_params` | F-007/F-014 | S |
| `When "executing control query:"` alias | F-007 | S |

**Why first:** the log turns 3175 opaque fails into measurable buckets and replaces
every estimate below with a count; parameters unlocks 62 skips (many pass
immediately). **Exit criterion:** a committed `OPENCYPHER_TCK_FAILURES.jsonl` and a
re-measured baseline with per-bucket counts. *Re-scope Phases 1–6 against real
buckets before committing effort.*

**Est. recovery:** +parameters passes (tens); metric trust ≫ its point value.

---

## Phase 1 — High-yield quick wins · **S→M** · *low coupling, parallelisable*

| Item | Ref | Band | Category impact |
|---|---|---|---|
| DELETE empty-result (phantom-row) fix | F-039 | S | `delete` ~0→~90% |
| Aggregate/expression column-name fidelity (verbatim) | F-005/F-011 | S | return, return-orderby, aggregation, literals |
| Side-effect counting semantics (overwrite +1/-1, distinct-label, null-prop) | F-041 | M | set, create, merge, remove, delete |
| Quantifier `pred(x IN list WHERE …)` parser form | F-031 | S-M | `quantifier` unblocks ~all 604 to *reach* eval |
| Bracket-less relationships `-->`/`--`/`<--` + `[:A\|:B]` | F-036 | M | match, match-where (~54) |
| Missing functions: `reverse(str)`, `startNode/endNode/properties`, `sign/rand/cot/haversin` | F-025 | S | string, graph, math |

**Why now:** each is a self-contained edit over machinery that already executes
correctly; none needs new infrastructure; several are `S` with outsized payoff.
**Est. recovery band: large** — this is the steepest part of the curve.

---

## Phase 2 — Three-valued logic + runtime type-guards · **M** · *W-B + W-A/A1*

One coherent change to the two evaluators (`eval/projection/core.rs` +
`eval/predicate.rs`): a shared 3VL logical helper returning `Bool|Null` (else a typed
`InvalidArgumentType` error), plus XOR, plus null/mixed-type comparison, plus
Kleene semantics for `IN`/`STARTS-ENDS-CONTAINS`/list-slice, plus quantifier
predicate null-propagation, plus `toX` invalid-type errors.

- **Refs:** F-009 (A1), F-010, F-021, F-022, F-024, F-026, F-031(b).
- **Category impact:** `boolean` ~4→~near-100%, plus large slices of `precedence`,
  `comparison`, `null`, `list`, `string`, `map`, `typeConversion`, and the
  quantifier tail. **The single highest scenario-per-effort item.**
- **Hard gate:** must not regress the Neo4j diff-suite (300/300) — see F-045.

---

## Phase 3 — Parser breadth · **M** · *W-E*

Numeric/string literals (hex/octal/scientific/underscore/`.5`, `\uXXXX`), `^`
right-assoc + precedence, `%` sign, comparison chaining, `SET n = {map}` replace +
`SET (n).prop`.

- **Refs:** F-013, F-023, F-040(a).
- **Category impact:** most of `literals` (76) and `precedence` (86), part of
  `string`/`math`, and the `Set4` family (with Phase 1's counting).

---

## Phase 4 — Static semantic-analysis pass · **L** · *W-A/A2, the negative-test mass*

A validate-after-parse stage over the AST emitting `OpenCypherErrorKind` + detail
tokens. Stage the checks by yield:

1. Variable scoping/type-conflict (`VariableTypeConflict`, `VariableAlreadyBound`,
   `UndefinedVariable`, `NoVariablesInScope`) — ~110+ in `match` alone.
2. Aggregation placement (`AmbiguousAggregationExpression`, `NestedAggregation`,
   agg-in-WHERE/ORDER BY, `NoExpressionAlias`).
3. SKIP/LIMIT arguments (`NegativeIntegerArgument`, float `InvalidArgumentType`,
   non-constant).
4. Projection/UNION structure (`ColumnNameConflict`, `DifferentColumnsInUnion`,
   `InvalidClauseComposition`) and write-side (`InvalidDelete`, MERGE `SemanticError`).

- **Refs:** F-009 (A2), F-035, F-040, F-041, F-042; token emission clears the F-006
  wording gate.
- **Category impact:** the ~241 negative-test scenarios in `match`, plus
  match-where, return-skip-limit, union, with-where, and the write-side compile-time
  errors. **The largest read-side lever.** Also fold in the `SemanticError`
  dead-variant fix (`error.rs`).

---

## Phase 5 — Temporal subsystem · **XL** · *W-D, internally staged*

The biggest single investment (`03-*`). Stage strictly:

1. **[XL, the gate]** Typed temporal value + canonical rendering + projection-boundary
   serialisation (breaks the duration-Object-vs-String wall). Nothing else in temporal
   is reliable until this lands.
2. **[L]** Full ISO-8601 parsing (dates + ISO duration with carry).
3. **[M each]** Property-access accessors (~50 names), comparison/ordering, arithmetic
   completion (×/÷, fractional, all-type ±duration), creation-from-map, projection-
   from-temporal, store/round-trip.
4. **[L]** Truncation (largest sub-block).
5. **[M, deferrable, hardest correctness]** Timezone database (promote `chrono-tz`/
   adopt `jiff`): named zones, DST, historical offsets, `epochMillis` — schedule
   **after** the tz-free slice (date/local*/duration), which is reachable without it.

Also unblocks the temporal-gated majority of `with-orderBy` (F-037) and complex-literal
CREATE setups.

---

## Phase 6 — Pattern predicates, EXISTS, path & counting semantics · **M→L**

- Pattern-predicate-in-WHERE graph-context wiring (F-033/G4).
- Pattern-comprehension `@tck_path` output shape (F-033/G5).
- `EXISTS { MATCH … WHERE … }` full subquery grammar + correlation (F-033/G6).
- Path null/zero-length + `length()` type errors (F-034/G7).
- Undirected/self-loop match-counting (F-034/G8).
- ORDER BY-expression re-evaluation + cross-type ordering (F-038).

Back-loaded because several depend on Phase 1 (column names) and Phase 2 (3VL) being
in place.

---

## Phase 7 — The long tail · *explicitly scoped, diminishing returns*

- **Non-finite floats** (NaN/±∞) — value-representation change (F-028), low count.
- **Procedure registration + CALL/YIELD** (F-042) — `L`, low product value.
- **Historical/named timezone offsets** (F-016) — the hardest temporal correctness.
- **Disputed/ambiguous scenarios** the reference implementation itself diverges on.
- **binary-tree fixtures + triadic selection** (F-007) — `S-M` harness + engine.

**This phase is where "100%" lives, and where effort-per-scenario is worst.** Treat
it as opportunistic, not a milestone.

---

## F-044 — Expected conformance trajectory (estimate, non-linear, overlapping)

| After phase | Rough band | Driver |
|---|---|---|
| 0 | ~14–16% | parameters unlock + trustworthy metric |
| 1 | ~25–35% | write quick wins + column names + quantifier-reachable + bracket-less rels |
| 2 | ~40–50% | 3VL/type-guards flip boolean + expression slices |
| 3 | ~45–55% | literals + precedence |
| 4 | ~55–68% | negative-test mass across match/write/union |
| 5 | ~72–85% | temporal (the single largest bucket) |
| 6 | ~78–88% | pattern/EXISTS/path/counting |
| 7 | asymptotic | tail, diminishing returns |

**These bands are directional.** They assume the Step-0 log confirms the bucket
sizes and that the overlaps play out as modelled. **Re-baseline after every phase.**

**Confidence.** Medium (magnitudes solid; exact percentages pending F-004).

---

## F-045 — Hard constraint: the Neo4j differential suite must not regress

Phases 2 and 4 introduce **strict typing and error-raising** where Nexus is currently
lenient. Internal tests and the 300/300 diff-suite may lean on lenient truthiness or
silent success. **Every phase that touches the evaluators or adds validation must gate
on the full workspace suite + the diff-suite before merge.** Keep WHERE-filter
truthiness (`null` → drop row) intact; make only logical **operators** and explicit
validation strict. This is the primary regression risk of the whole project.

**Confidence.** High.

---

## F-046 — Parallelisation & ownership

The six workstreams (`02-*`) are largely independent after Phase 0:

- **Parser breadth (W-E)** — one owner, self-contained edits (Phases 1 bracket-less,
  3 literals/precedence).
- **Evaluator (W-B + W-A/A1)** — one owner; serialise Phase 2 internally (the two
  evaluators must stay in lock-step).
- **Static analysis (W-A/A2)** — one owner; a new module, low coupling (Phase 4).
- **Temporal (W-D)** — one owner; the XL subsystem (Phase 5), the tz-free slice and
  tz DB are separable.
- **Result-shape/counting (W-C)** — folds into the write-clause owner (Phase 1).

Cross-cutting items — **column-name fidelity (W-C)** and **error-kind/token
classification (W-A)** — should be planned as **global workstreams**, not owned by
any single cluster, because their value spans many categories.

**Confidence.** High.

---

## F-047 — Recommendation: adopt a conformance SLO, not "100%", and add a CI regression gate

**100% is the wrong target** (F-003, Phase 7). Recommended framing:

1. **Target the 70–85% band** via Phases 0–5; declare that the conformance goal.
2. Once past a chosen threshold (e.g. 60%), **wire the TCK run into CI as a
   ratchet** — fail the build if the pass count drops — so hard-won conformance
   cannot silently regress (the diff-suite already guards happy-path Neo4j
   agreement; this guards spec depth).
3. Keep the **honest two-number story** in `CLAUDE.md`/README: Neo4j diff-suite
   (curated agreement) *and* TCK conformance (strict spec). They measure different
   things and both belong.

**Confidence.** High.

---

## F-048 — Materialisation

Each phase maps cleanly onto rulebook tasks/specs. Suggested first tickets, in order:

1. `phaseNN_tck-failure-instrumentation` — Phase 0 log + parameters step (F-004/F-007).
2. `phaseNN_tck-delete-empty-result` — Phase 1, the single highest-ROI `S` fix (F-039).
3. `phaseNN_tck-column-name-fidelity` — Phase 1 global W-C (F-005/F-011).
4. `phaseNN_tck-side-effect-counting` — Phase 1 W-C (F-041).
5. `phaseNN_tck-quantifier-in-where` + `phaseNN_tck-bracketless-rels` — Phase 1 W-E.
6. `phaseNN_tck-three-valued-logic` — Phase 2, the top expression lever (F-021/F-010).
7. `phaseNN_tck-semantic-analysis-pass` — Phase 4, the read-side lever (F-009 A2).
8. `phaseNN_tck-temporal-typed-value` … (staged per `03-*`) — Phase 5.

Per project convention, drive these through `rulebook_task` with the checklist order
reflecting the dependency graph above. **Recommend materialising Phases 0–1 first**
(the measurement foundation + quick wins), re-baselining against the Step-0 buckets,
then committing to Phases 2–5.

**Confidence.** High on structure; exact ticket scope should follow the Phase-0
re-baseline.
