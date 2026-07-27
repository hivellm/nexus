# Proposal: phase0_fix-match-create-rel-aggregating-return-dropped

## Why

Silent data loss discovered by manual testing of a running 3.0.0 server:
`MATCH (a:L{k:1}),(b:L{k:2}) CREATE (a)-[:R]->(b) RETURN count(*)` reports success
(and `count = 1`) but the relationship is **never created** — the response even
omits the `stats` block. The same query with a non-aggregating projection
(`RETURN a.k`) works. A user counting the rows they just wrote (a common bulk
relationship-creation idiom) silently loses every edge.

Root cause: the planner inserts the `Create` operator before the projection sink,
but the sink-search only matched `Operator::Project`. When the RETURN/WITH
aggregates, the sink is `Operator::Aggregate`, so `Create` was appended AFTER it.
The Aggregate runs first, collapses the rows and overwrites `context.variables`,
destroying the MATCH-bound node objects `a`/`b` (their `_nexus_id`). `Create` then
runs against zero usable rows and skips the write
(`executor/operators/create.rs` ~1046-1067).

## What Changes

`crates/nexus-core/src/executor/planner/queries/planner_core.rs` (~851-857): widen
the sink-position predicate from `Operator::Project { .. }` to
`Operator::Project { .. } | Operator::Aggregate { .. }`, so `Create` is inserted
before ANY projection sink. This mirrors the already-correct WITH-insertion
predicate at planner_core.rs:793.

## Impact
- Affected specs: none (executor internals).
- Affected code: crates/nexus-core/src/executor/planner/queries/planner_core.rs
  (one predicate); new regression test
  crates/nexus-core/tests/regression/match_create_rel_aggregating_return_test.rs.
- Breaking change: NO (fixes silent data loss; no behavior a correct program relied on).
- User benefit: MATCH + CREATE relationship + aggregating RETURN/WITH now persists
  the relationship.

## Status
Implemented and verified: new regression tests (3) pass; full `regression` group
208/0; clippy + fmt clean. Pending CHANGELOG entry + commit.
