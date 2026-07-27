# 05 — Quantifier, Patterns & Paths

Covers `expressions/quantifier` (604, 2.6%), `expressions/pattern` (50, 0%),
`expressions/path` (7, 0%), `expressions/existentialSubqueries` (10, 10%),
`useCases/countingSubgraphMatches` (11, 0%). Headline: this "665-scenario XL QPP
cluster" is a **misnomer** — it is four independent, mostly-`M` correctness/wiring
fixes on already-built foundations, plus the two cross-cutting workstreams (W-A/A1
type-checks, W-C column names).

---

## F-029 — `expressions/quantifier` is list predicates, NOT quantified path patterns

**Evidence.** Every `Quantifier{1..12}.feature` tests the list-predicate functions
`none()/single()/any()/all()` in the `pred(x IN list WHERE cond)` form
(`Quantifier3.feature:48` `any(x IN <list> WHERE <condition>)`); feature titles are
"None/Single/Any/All quantifier" + interop/invariants. This is **not** the GQL/
Cypher-5 quantified path pattern `(...){1,3}`.

**Impact.** The "#2 gap overall" is **not** a net-new traversal engine. It is
predominantly a parser-wiring fix (F-031). Re-scoping this alone removes the single
biggest sizing error a naive read of the report would produce.

**Confidence.** High (feature titles + step forms verified).

---

## F-030 — The real QPP engine is already substantially built (and barely tested here)

**Evidence.** `Operator::QuantifiedExpand` (`executor/types.rs:595-636`), a ~700-line
executor (`operators/quantified_expand.rs`) with per-position list promotion, inner
`WHERE`, path-modes WALK/TRAIL/ACYCLIC/SIMPLE and cycle policy; a planner builder
(`planner/queries/qpp.rs:107-257`); parser support (`parser/clauses/pattern.rs:151`
`parse_quantified_group`); and its own curated suite
(`tests/compatibility/qpp_tck_scenarios.rs`, 30+ passing). The vendored TCK corpus
predates Cypher-25 QPP syntax and hardly exercises it.

**Impact.** **Leave the QPP subsystem as-is** for TCK purposes — it is not where the
quantifier failures come from. No XL engine hides in this cluster.

**Confidence.** High.

---

## F-031 — Quantifier root cause: no `IN…WHERE` parser form for the four predicate names

**Evidence.** Only `filter()` (`parser/expressions/identifier.rs:78`) and list
comprehensions get the special `IN … WHERE` handling. `any/all/none/single` fall
through to normal argument parsing (`identifier.rs:161-163`); the `IN` operator parse
stops before `WHERE` (`precedence.rs:228-237`), leaving a dangling `WHERE` that the
arg loop then tries to parse as an expression → the query errors → the harness counts
it FAIL (`tck_opencypher.rs:232`). The evaluator arms already exist and expect the
3-arg shape (`fn_list.rs:319-473`).

**Impact.** This one **S-M** parser fix (mirror the `filter()` block) unblocks
**~all 604**. Then two follow-ons: **(a)** Kleene 3VL — the evaluator uses
`as_bool().unwrap_or(false)` (`fn_list.rs:347/385/423/462`), collapsing NULL→false
(W-B, **M**, ~200-300 of 604); **(b)** compile-time argument type-checks for the
`Fail…InvalidArgumentType` outlines (W-A, rides A1).

**Confidence.** High (the dangling-`WHERE` failure path is precise).

---

## F-032 — Variable-length paths & `shortestPath` are mature; not the gap

**Evidence.** `execute_variable_length_path` (`operators/path.rs:455`),
`find_shortest_path` (`:737`), `find_all_shortest_paths` (`:813`); parser handles
`[*]`, `[*1..3]`, `[*2]`, `[*0..]` (`parser/clauses/pattern.rs:591-603`);
`shortestPath`/`allShortestPaths` special-cased (`parser/expressions/identifier.rs:134`).

**Impact.** No work here for its own sake. (Note the bracket-**less** relationship gap
`-->` is a *different*, real parser hole — see F-036.)

**Confidence.** High.

---

## F-033 — Pattern predicates / EXISTS / comprehensions: parsed, partially evaluated, ~0 pass

**Evidence.** AST + parsing + partial eval all exist: `Expression::Exists`
(`ast.rs:1154`), `PatternComprehension` (`ast.rs:1199`); `EXISTS { … }`
(`parser/expressions/structured.rs:188`); pattern comprehension `[p = (n)-->() | e]`
(`literals.rs:60`); negated pattern predicate `NOT (n)-->()` → Exists
(`precedence.rs:66`); eval via `check_pattern_exists` (`core.rs:481-486`) and
comprehension (`core.rs:640`). **But** `helpers.rs:840/851` short-circuit
`Exists`/`PatternComprehension` to `false` when there is no graph context, and
QPP-inside-EXISTS errors (`core.rs:520`).

**Impact.** Three fixes: **(G4, M)** correct existential eval in WHERE — anonymous
rel/node, undirected, var-length `(n)-[:T*]-()`, negation/conjunction, graph-context
wiring (`helpers.rs:840`); **(G5, M)** pattern-comprehension output must emit proper
path values matching the `@tck_path` shape; **(G6, M-L)** upgrade the single-pattern
`Exists` to a full `exists { MATCH … WHERE … }` subquery with correlation (clears 9
of 10 existentialSubqueries). `expressions/pattern` (50) also contains negative tests
needing `Fail…UnexpectedSyntax` (W-A).

**Confidence.** High on direction; per-scenario shape (pattern comprehension vs
`@tck_path`) medium until the Step-0 log confirms.

---

## F-034 — path (0%) is null-path handling, not traversal; counting is match-count semantics

**Evidence.** `expressions/path` fails on `nodes(null)`/`relationships(null)` →
should be `null`, `length()` type errors, and exact un-aliased column names
(`nodes(p)`) — **not** var-length (which works, F-032). `useCases/countingSubgraphMatches`
(0%) fails uniformly on undirected/self-loop match-counting semantics (a self-loop
counted once; a directed edge under `--` counted twice) and/or `count(*)` column
naming (F-005).

**Impact.** **(G7, S-M)** path-value edge semantics (null/zero-length/`length()`
type error) + **(G8, S-M)** undirected/self-loop counting. Both depend on the shared
column-name fix (W-C, F-011) to stop failing on the header before the value is even
compared.

**Confidence.** High for path; counting root cause high-level (self-loop vs naming)
pending the Step-0 log.

---

## Ordered sub-workstreams (this cluster)

1. **[S, shared] Column-name fidelity** (W-C / F-011) — prerequisite for counting +
   path scenarios to pass at all; pays off far beyond this cluster.
2. **[S-M] Quantifier `IN…WHERE` parser form** (F-031) — unblocks ~all 604. Highest
   single lever in the cluster.
3. **[M] Kleene 3VL for list predicates** (F-031, W-B) — clears the bulk of the
   remaining quantifier fails.
4. **[S-M] Undirected/self-loop counting** (F-034/G8) — depends on #1.
5. **[S-M] Path null/zero-length + `length()` semantics** (F-034/G7) — depends on #1;
   shares path-value shape with #6.
6. **[M] Pattern-predicate-in-WHERE** (F-033/G4) — graph-context wiring.
7. **[M] Pattern comprehension path-value output** (F-033/G5).
8. **[M-L] EXISTS full subquery grammar** (F-033/G6) — most independent; parallelisable.
9. **[rides W-A/A1] Argument type-checks** for the `Fail…InvalidArgumentType` subset.

**Do not** budget this cluster as XL — it is **M-L**, and two of its biggest items
(column names, type-checks) are cross-cutting workstreams whose value extends well
past these five categories, so account for them globally (`02-*`), not here.
