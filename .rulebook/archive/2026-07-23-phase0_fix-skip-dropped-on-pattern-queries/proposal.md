# Proposal: phase0_fix-skip-dropped-on-pattern-queries

## Why

`SKIP` is silently ignored on pattern-driven queries: `MATCH (n) RETURN
labels(n)[0] AS label ORDER BY label SKIP 1` returns ALL rows (SKIP dropped;
LIMIT and ORDER BY apply). Confirmed 2026-07-21/22 by a control test during
the pattern-less ORDER BY fix (commit f95d4458): `Clause::Skip` had no
planner handling anywhere; f95d4458 added `Operator::Skip` and wired it for
the PATTERN-LESS branches only (procedure YIELD, bare RETURN). The
pattern-based path (`plan_execution_strategy`) still never consumes a SKIP
clause — a system-wide, pre-existing gap. Silent wrong results for any
paginated MATCH query.

Groundwork already in place from f95d4458: `Operator::Skip { count }` exists
with execution (`executor/operators/project.rs::execute_skip`), dispatch arms
(both exhaustive matches), and a cost arm. This task threads it through
`plan_execution_strategy` (`executor/planner/queries/planner_core.rs`) and
its aggregation/UNION/WITH sub-paths in the standard openCypher order
(Sort -> Skip -> Limit), which is the larger blast radius deliberately left
out of the surgical f95d4458 fix. See also the recorded anti-pattern:
`.rulebook/knowledge/anti-patterns/planner-no-pattern-branch-must-independently-apply-order-by-skip-limit.md`.

## What Changes

- Failing tests first: MATCH ... ORDER BY ... SKIP n (with/without LIMIT),
  SKIP with aggregation, SKIP under WITH pipelines, SKIP with UNION — pin
  today's silent-drop behavior, then fix.
- Thread `skip_count` into `plan_execution_strategy` and every sub-path that
  currently emits Sort/Limit, preserving Sort -> Skip -> Limit order.
- Audit for other silently-unhandled clauses in the same collection loop
  while there (report, don't necessarily fix).

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (SKIP documented semantics)
- Affected code: `crates/nexus-core/src/executor/planner/queries/planner_core.rs`
  (+ sub-path modules it delegates to)
- Breaking change: NO — corrects silently wrong results
- User benefit: pagination (`SKIP`/`LIMIT`) works on MATCH queries as
  documented and as Neo4j behaves
