# Tasks: phase0_fix-skip-dropped-on-pattern-queries

`MATCH ... ORDER BY ... SKIP n` silently ignores SKIP (LIMIT/ORDER BY apply).
`Operator::Skip` exists since f95d4458 but only pattern-less branches emit it.
Details in proposal.md.

## 1. Root cause and fix
- [x] 1.1 Failing tests first: SKIP on plain MATCH, MATCH+ORDER BY,
      MATCH+aggregation, WITH pipelines, UNION — pin the silent drop today
      (tests/cypher/skip_pattern_queries_test.rs; confirmed SKIP cases failed
      pre-fix, controls passed)
- [x] 1.2 Thread skip_count through plan_execution_strategy and its sub-paths
      (Sort -> Skip -> Limit order); audit the clause-collection loop for any
      other silently-unhandled clauses and report findings. Threaded skip_count
      into plan_execution_strategy (strategy.rs) + emit Skip between Sort/Limit;
      added post_union_skip extraction + Skip emission in the UNION branch
      (planner_core.rs). Audit findings: (a) post-UNION clause loop still drops
      chained further UNIONs after the first (pre-existing, out of scope);
      (b) a SKIP/LIMIT after UNION with no ORDER BY binds to the right-hand arm
      (openCypher clause attachment, documented, not a bug).
- [x] 1.3 Regression tests green; verify the pattern-less path (f95d4458
      tests) is unaffected (test_call_procedures.rs still green; cypher group
      386 passed, executor 223 passed)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
      (docs/specs/cypher-subset.md SKIP section rewritten; CHANGELOG [3.0.0])
- [x] 2.2 Write tests covering the new behavior
      (tests/cypher/skip_pattern_queries_test.rs — 8 tests)
- [x] 2.3 Run tests and confirm they pass
