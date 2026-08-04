## 1. Implementation

> **The proposal was wrong twice, and both errors were caught by measuring rather
> than reasoning.** It claimed `useCases/countingSubgraphMatches` was at 0% (it was
> at 9/11, 81.8% — only two scenarios failed) and that the defect was self-loops
> being counted twice in undirected traversal generally. Deriving the expected
> counts from node degree and then probing directly showed a different split,
> recorded below.

- [x] 1.1 self-loop counted once
  - Real scope: only the **source-less** relationship scan in
    `executor/operators/expand.rs` (the path a fully-anonymous slot like `()-[]-…`
    takes) emitted each relationship once per direction under `Direction::Both`.
    For a self-loop the two orientations are the *same* binding — source and target
    are the same node — so it was counted twice, inflating every undirected count
    over a looping node. Now emitted once.
  - The **anchored** path (a bound source node) was already correct: the control
    `MATCH (l:Looper)--()` returned 1 before any change. That is the half of the
    proposal's claim that was false, and it is why the fix is scoped to one branch
    rather than to undirected traversal in general.
  - Located by arithmetic, not guesswork: TCK [11] can only total 6 if `deg(l)=3`,
    which requires the loop to contribute one incidence, not two.
- [ ] 1.2 undirected counting semantics — **MOVED, not done.** See below.

### Why 1.2 moved to `phase21_tck-consecutive-relationship-match-clauses`

Probing showed the two failing scenarios are not an undirected-counting defect at
all: **relationship isomorphism is entirely absent from the `MATCH`/`Expand`
chain**. It exists for `EXISTS` and pattern comprehensions only. On a
one-relationship graph, `MATCH (x)-[r1]-(y)-[r2]-(z)` returns 2 instead of 0 — with
named slots, anonymous slots, or even the same variable repeated. Naming changes
nothing; the comparison does not exist.

An implementation was attempted here: a row-borne accumulator of consumed
relationship ids, relying on the fact that a pattern starting with a node scan gets
fresh rows to reset the scope implicitly. It made both TCK scenarios pass
(`countingSubgraphMatches` 9/11 → 11/11, overall 1817 → 1820) **and regressed
`clauses/match-where` 28 → 25**. It was reverted rather than shipped.

The regression is real, not corpus noise — established by A/B, since this corpus has
a documented run-to-run variance band: two **identical** runs both reported
`match-where` at 25 while the grand total moved only 1820 → 1819. Root cause: a
clause continuing from an already-bound variable emits no scan, so it inherits the
previous clause's ids and the rule is applied where openCypher does not ask for it.
`OPTIONAL MATCH` then makes it worse than a missing row — when every candidate is
rejected, the optional-padding branch adds an all-null row, so the count goes *up*,
which is why it surfaced as an unrelated-looking over-count.

Fixing it properly needs an **explicit** pattern scope (a pattern id on
`Operator::Expand`, 11 sites across 7 files) rather than one that depends on where
scans happen to occur. That is a larger change than this task, and it shares a
mechanism with two other verified defects found by the same probes: a second `MATCH`
clause containing a relationship returns zero rows, and comma-separated parts of one
`MATCH` do not enforce isomorphism. All of it — reproductions, expected values and
the regression trap — is written up in
`phase21_tck-consecutive-relationship-match-clauses`.

TCK `useCases/countingSubgraphMatches` therefore remains at 9/11 after this task.
The self-loop fix is necessary for those two scenarios but not sufficient.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md`: undirected matching over a self-loop counts it
    once, and the still-missing isomorphism rule is stated as a known gap pointing
    at the follow-up task, so the spec does not imply conformance Nexus lacks.
- [x] 2.2 Write tests covering the new behavior
  - 5 tests in `tests/cypher/undirected_self_loop_counting_test.rs`: the source-less
    undirected self-loop count, plus controls that a normal edge still yields both
    orientations, that the directed form is untouched, that the anchored path still
    counts a loop once, and that a mixed graph totals 5 single-slot bindings
    rather than 6.
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 631, `--test executor` 272, `--test regression` 227,
    `--test compatibility` 245 — 0 failures.
  - Full `--workspace --no-fail-fast`, `fmt` and `clippy`: see the commit message.
  - **TCK: no attributable movement.** `countingSubgraphMatches` stays 9/11 (its two
    scenarios need the isomorphism work that moved out of this task) and
    `clauses/match-where` is confirmed back at 28, which is what proves the reverted
    accumulator — not something else — caused that regression. The grand total reads
    1820 vs the 1817 at the previous commit, but that is corpus variance and is NOT
    claimed as a gain: the difference sits in `clauses/with-orderBy` (+2) and
    `clauses/return` (+1), and `with-orderBy` alone measured 90 → 91 → 92 across
    three runs of three different builds while nothing in this change touches
    `ORDER BY`. Treat this task's TCK delta as zero.
