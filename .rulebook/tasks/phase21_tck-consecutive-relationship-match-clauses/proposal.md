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

A second `MATCH` whose pattern is node-only works, and the comma form works, but a
second `MATCH` whose pattern contains a relationship yields nothing. This is a
cartesian-product / binding-materialization defect in clause composition,
independent of relationship isomorphism: with a single relationship in the graph
there is nothing for an isomorphism rule to reject, and openCypher does not apply
that rule across separate `MATCH` clauses anyway.

Likely related: the recorded `Expand` required-partial-binding leak, and
`phase21_tck-comma-pattern-binding-materialization` (still pending) — read that
task's write-up first, since the two may share a root cause.

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
