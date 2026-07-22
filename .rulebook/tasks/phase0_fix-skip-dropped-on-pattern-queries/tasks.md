# Tasks: phase0_fix-skip-dropped-on-pattern-queries

`MATCH ... ORDER BY ... SKIP n` silently ignores SKIP (LIMIT/ORDER BY apply).
`Operator::Skip` exists since f95d4458 but only pattern-less branches emit it.
Details in proposal.md.

## 1. Root cause and fix
- [ ] 1.1 Failing tests first: SKIP on plain MATCH, MATCH+ORDER BY,
      MATCH+aggregation, WITH pipelines, UNION — pin the silent drop today
- [ ] 1.2 Thread skip_count through plan_execution_strategy and its sub-paths
      (Sort -> Skip -> Limit order); audit the clause-collection loop for any
      other silently-unhandled clauses and report findings
- [ ] 1.3 Regression tests green; verify the pattern-less path (f95d4458
      tests) is unaffected

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
