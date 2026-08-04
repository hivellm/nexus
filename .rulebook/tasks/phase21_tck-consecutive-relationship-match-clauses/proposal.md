# Proposal: phase21_tck-consecutive-relationship-match-clauses

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Surfaced while implementing `phase21_tck-undirected-self-loop-counting`, whose
> probes exposed three adjacent defects that were deliberately left out of that
> task's scope rather than silently widened into it.

## Why

Three verified defects in how `MATCH` clauses and pattern parts compose. Each was
reproduced directly, not inferred; the exact probe and the observed value are
recorded per defect so the next session does not have to re-derive them.

### D1 — A second `MATCH` clause containing a relationship returns zero rows

Fixture: `CREATE (a:A)-[:T]->(b:B)` (one node pair, one relationship).

| Query | Observed | Correct |
|---|---:|---:|
| `MATCH (x)-[r1]->(y) RETURN count(*)` | 1 | 1 |
| `MATCH (x)-[r1]->(y) MATCH (p) RETURN count(*)` | 1 | 1 |
| `MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q) RETURN count(*)` | **0** | **1** |
| `MATCH (x)-[r1]->(y), (p)-[r2]->(q) RETURN count(*)` | 1 | 0 (see D2) |

**Root cause (found — item 1.1 is complete).** `EXPLAIN` on the failing query gives:

```
AllNodesScan { variable: "x" }
Expand { source_var: "x", target_var: "y", rel_var: "r1" }
Expand { source_var: "p", target_var: "q", rel_var: "r2" }   <- nothing binds "p"
Aggregate
```

No scan is emitted for `p`, so `execute_expand` finds no `source_var` in any
incoming row, `extract_entity_id` fails, and every row is dropped — hence zero rows.

The omission is in the additional-pattern loop of
`executor/planner/queries/strategy/pattern_lowering.rs`: the driving scan is emitted
**only inside `if !node.labels.is_empty()`**, and there is no `else`. An *unlabeled*
node in a second (or comma) pattern therefore gets no scan at all. The first
pattern's lowering does emit `AllNodesScan` for an unlabeled node — the two paths
disagree. That is also why the row-count table above is misleading in one place:
`MATCH (x)-[r1]->(y) MATCH (p) RETURN count(*)` returns 1 not because it works, but
because `p` is left **unbound** and projects as `Null`; the correct answer on this
fixture is 2 rows (the cartesian with both nodes). Verified:
`MATCH (x)-[r1]->(y) MATCH (p) RETURN x, p` → one row, `p = Null`.

**Fix: emit the missing scan. That was sufficient — CORRECTING an earlier claim in
this file.** An earlier revision of this proposal asserted that emitting the scan
would decorrelate the rows, because `AllNodesScan`'s dispatch clears
`result_set.rows` and rebuilds them via `materialize_rows_from_variables`. That was
wrong, and it was wrong from reading the code instead of running it.
`context.variables` is a **columnar, row-aligned** set: `apply_cartesian_product`
expands every existing column in lockstep with the new one, so materialising rows
from it is a zip and correlation is preserved.

Verified after the fix on the one-relationship fixture:
`MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q) RETURN x, y, p, q` returns exactly one row
with `x = p = :A` and `y = q = :B`; `MATCH (x)-[r1]->(y) MATCH (p) RETURN x, p`
returns the 2-row cartesian with `x` bound in both. So there was **no dependency on
`phase21_tck-comma-pattern-binding-materialization`** for D1 — that suspicion, twice
recorded here, was unfounded. Fixed by adding the missing `AllNodesScan` for an
unlabelled node in the additional-pattern loop, guarded so a variable an earlier
pattern already bound is never rescanned.

Still possibly related for the *comma* task's own OPTIONAL-MATCH-rebinding symptom:
the recorded `Expand` required-partial-binding leak.

### D2 — Relationship isomorphism is not enforced across comma-separated parts

openCypher scopes relationship isomorphism to a whole `MATCH` clause, including its
comma-separated pattern parts. On the same one-relationship fixture,
`MATCH (x)-[r1]->(y), (p)-[r2]->(q) RETURN count(*)` returns **1**; the correct
answer is **0**, because `r1` and `r2` would have to be the same relationship.

`phase21_tck-undirected-self-loop-counting` added enforcement along a single path
pattern (an accumulator of consumed relationship ids on the row, in
`executor/operators/expand.rs`). Comma parts are not covered: each part begins with
its own node scan, and a scan builds fresh rows, so the accumulator restarts and
the second part cannot see what the first consumed.

### D0 — Relationship isomorphism is missing from the `MATCH` pipeline entirely

**The core defect, and currently UNFIXED.** Cypher requires that within one
pattern, two relationship slots never bind the same relationship. It is
implemented for `EXISTS` (`executor/eval/helpers/exists.rs`) and pattern
comprehensions, but **not** for the `MATCH`/`Expand` chain. On the same
one-relationship fixture:

| Query | Observed | Correct |
|---|---:|---:|
| `MATCH (x)-[]-(y)-[]-(z) RETURN count(*)` | 2 | 0 |
| `MATCH (x)-[r1]-(y)-[r2]-(z) RETURN count(*)` | 2 | 0 |
| `MATCH (x)-[r]-(y)-[r]-(z) RETURN count(*)` | 2 | see 1.6 |

Naming the slots changes nothing — the comparison does not exist. This is what
blocks TCK `useCases/countingSubgraphMatches` [10] and [11]. With fixture
`CREATE (:A)-[:T1]->(l:Looper), (l)-[:LOOP]->(l), (l)-[:T2]->(:B)`, giving
`deg(A)=1, deg(l)=3, deg(B)=1`:

| Query | Observed | Correct | Arithmetic |
|---|---:|---:|---|
| `MATCH (:A)-->()--() RETURN count(*)` | 3 | 2 | the 3rd binding walks `--` back over the `-->` edge |
| `MATCH ()-[]-()-[]-() RETURN count(*)` | 11 | 6 | `1+9+1` without distinctness vs `0+(3*2)+0` with |

### D3 — Scoping the accumulator is the hard part: a naive row accumulator REGRESSES

A first attempt stored the consumed relationship ids on the row and let the scope
reset implicitly, since a pattern beginning with a node scan gets fresh rows. It
made [10] and [11] pass (`countingSubgraphMatches` 9/11 → 11/11) **and regressed
`clauses/match-where` 28 → 25**, so it was reverted rather than shipped.

Root cause of that regression — the trap to avoid: a clause continuing from an
already-bound variable emits **no scan**, so it inherits the previous clause's ids
and isomorphism is applied where openCypher does not ask for it.
`MATCH (a)-->(b) WHERE b:B OPTIONAL MATCH (a)-->(c) WHERE c:C` is the shape, and
`OPTIONAL MATCH` makes it worse than a missing row: when every candidate is
rejected, the optional-padding branch emits an extra all-null row, so the count
goes **up**. It surfaced as "2 rows where 1 was expected", which reads like an
unrelated over-count.

That regression was almost dismissed as the corpus's known run-to-run variance. It
is not, and the attribution is now closed on both sides: two **identical** TCK runs
both reported `match-where` at 25 while the grand total moved only 1820 → 1819, and
reverting the accumulator put `match-where` back to exactly 28. Always A/B a
suspicious category against an identical re-run before accepting or dismissing it.

Calibration for reading this corpus: the variance is category-specific, not a flat
band on the total. `clauses/with-orderBy` measured 90 → 91 → 92 across three runs of
three different builds, none of which touched `ORDER BY`, so a ±2 swing there means
nothing; `clauses/match-where` reproduced exactly across identical runs, so a 3-point
move there means everything. Judge a category against its own re-run, never against
the total.

So the scope must be **explicit**, never a side effect of where scans happen: carry
a pattern/clause id on `Operator::Expand` and key the accumulator on it. That fixes
D3 by construction and gives D2 its mechanism for free (comma parts of one clause
share the id).

### Already fixed, for context

The sibling defect — the source-less undirected scan emitting a self-loop twice,
since its two orientations are the same binding — was fixed and shipped separately
with `tests/cypher/undirected_self_loop_counting_test.rs`. The correct counts above
already assume it. It is necessary but not sufficient for [10]/[11].

## What Changes

- Carry a pattern-scope id on `Operator::Expand` (11 construction/destructuring
  sites across 7 files, mechanical) and key the isomorphism accumulator on it,
  replacing the implicit scan-boundary reset.
- Comma-separated parts of one `MATCH` share that id, so isomorphism spans them.
- Separate `MATCH` clauses get distinct ids, so the rule does not leak across them.
- Root-cause and fix the zero-row result for a second `MATCH` clause whose pattern
  contains a relationship.

## Impact

- Affected code: `crates/nexus-core/src/executor/operators/expand.rs`,
  `crates/nexus-core/src/executor/types.rs` (the `Operator` enum), planner lowering
  in `executor/planner/queries/relationships.rs` and `cost.rs`, dispatch in
  `executor/dispatch/operator_loop.rs` and `executor/operators/dispatch.rs`, plus
  three planner test modules that build `Operator::Expand` literals.
- Affected specs: `docs/specs/cypher-subset.md` — the isomorphism note added by the
  self-loop task states the current implicit scope and must be updated once the
  scope becomes explicit.
- Breaking change: NO for correct queries. D2's fix REDUCES result counts for comma
  patterns that currently reuse a relationship. That is a correctness fix, but any
  existing test asserting the inflated count must be updated deliberately and
  called out, the way `test_path_functions_with_null` was.
- Dependencies: read `phase21_tck-comma-pattern-binding-materialization` first; D1
  may share its root cause, and doing both together may be cheaper than sequencing.
- User benefit: multi-clause and comma-separated `MATCH` stop silently returning
  wrong row counts — zero rows in D1's case, which is the worst kind of silent
  wrong answer, and inflated counts in D2's.
