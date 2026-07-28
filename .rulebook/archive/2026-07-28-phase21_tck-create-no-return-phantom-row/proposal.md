# Proposal: phase21_tck-create-no-return-phantom-row

## Why
Write-only statements must return zero rows under openCypher, but the
executor's standalone-CREATE fast path synthesized a result row from named
CREATE variables even with no RETURN clause. This blocked 10 TCK
clauses/create scenarios ("Then the result should be empty — got 1 rows")
whose side effects were already correct after the cross-clause binding fix,
and the same bug class was suspected for other write families.

## What Changes
Gate result-row synthesis in both CREATE execution paths (the standalone fast
path in executor/dispatch/operator_loop.rs and execute_create_with_context in
executor/operators/create.rs) on an actual downstream row consumer
(Project/With/Aggregate). Probe DELETE/SET/REMOVE/MERGE without RETURN for the
same mechanism and pin whichever are already correct with regression tests.

## Impact
- Affected specs: docs/specs/cypher-subset.md result semantics (write-only statements)
- Affected code: executor/dispatch/operator_loop.rs, executor/operators/create.rs, executor/operators/dispatch.rs, regression tests
- Breaking change: NO (removes non-conformant phantom output; clients reading rows[0] of write-only statements were reading garbage)
- User benefit: openCypher-conformant empty results for write-only statements; CREATE ... RETURN count(*) returns the count instead of a phantom node row.

(Backfilled after archive — the task was executed and completed in commit 91cdd606.)
