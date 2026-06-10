# Proposal: phase6_call-in-tx-result-cap

Source: GitHub issue #22 (https://github.com/hivellm/nexus/issues/22)

## Why
The #12 fix made `CALL { subquery } IN TRANSACTIONS OF n ROWS` run the
subquery once and commit in a single transaction
(`crates/nexus-core/src/engine/mod.rs:4998`), correctly removing the prior
infinite loop. But `_batch_size` is now ignored and the whole subquery
result is materialized in memory (`all_results.extend(...)`) before one
commit; the `OF n ROWS` commit-granularity semantics are unimplemented.
For a subquery returning millions of rows this allocates the full result
set in-heap and holds all write locks for the entire subquery — an OOM /
long-lock-hold risk.

## What Changes
- Short term (this task): cap the materialized result count and return a
  structured error (e.g. `ERR_CALL_IN_TX_RESULT_TOO_LARGE`) instead of
  silently OOMing; document `OF n ROWS` as not-yet-implemented commit
  granularity.
- Leave proper per-N-rows batched commit (which needs planner operator
  support) as a separate, larger follow-up.

## Impact
- Affected specs: cypher-subset / CALL IN TRANSACTIONS
- Affected code: `crates/nexus-core/src/engine/mod.rs`
  (`execute_call_subquery_commands`)
- Breaking change: NO (turns a potential OOM into a clear, bounded error)
- User benefit: large `CALL IN TRANSACTIONS` subqueries fail fast with a
  clear error instead of OOMing the server.

## Notes
- Audit finding #9 (follow-up to #12). Scoped to the cap+error guard;
  true batched commit is a separate task.
