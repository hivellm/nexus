# 15. Fix sustained-write 100% CPU: CALL IN TRANSACTIONS infinite loop + O(N)-per-write label stats

**Status**: proposed
**Date**: 2026-06-08
**Related Tasks**: phase6_fix-sustained-write-busyloop, phase6_fix-unwind-write-persists

## Context

Under sustained write replay (backfill: thousands of MERGE/SET + edge MERGEs, indexes engaging) Nexus 2.3.1 hit 100% CPU with no Cypher query executing, became fully unresponsive (even RETURN 1 timed out), and only a restart cleared it (issue #12). A code hunt found two compounding causes; the exact production stall is load-dependent but the primary cause is a deterministic infinite loop.

## Decision

Fix two root causes. (1) execute_call_subquery_commands: `CALL { subquery } IN TRANSACTIONS OF n ROWS` re-executed the whole subquery against the same dataset every loop iteration; its termination check (all_results.len() < batch_count*batch_size, plus break-on-empty) was never satisfied when the subquery returned >= n stable rows, so it looped forever holding the engine write lock at 100% CPU with no per-iteration / active-query log — matching the reported signature exactly (a wedged worker blocks every other request incl. RETURN 1). IN TRANSACTIONS controls commit granularity, not re-execution, so the fix runs the subquery once in a transaction and commits. (2) LabelIndex::recompute_stats rebuilt a HashSet of all node ids across all label bitmaps (O(total node entries)) on every add_node/remove_node; the stats are diagnostic-only, so they are now computed lazily in get_stats/health_check and the eager per-write update was removed (the stored stats field was dropped). (3) Telemetry: find_relationship_between counts chain-walk hops and warns past 1000 when the exact-edge index misses, surfacing the O(degree) hub edge-MERGE pathology (the remaining candidate) as an observable warning instead of an opaque stall.

## Alternatives Considered

- Implement true paginated CALL IN TRANSACTIONS batching with a cursor (rejected for this fix: larger; single-transaction execution is correct and removes the loop; pagination can come later)
- Maintain LabelIndex total_nodes incrementally with atomics (rejected: unique-across-labels count for multi-label nodes is not trivially incremental; lazy-on-read is simpler and stats are not hot)
- Add a full /debug/threads stack-dump endpoint (rejected for this pass: the root cause was found by code analysis; a targeted chain-walk warning is the actionable telemetry)

## Consequences

CALL IN TRANSACTIONS terminates (deterministic regression test that would hang on the old code). Per-write CPU under sustained load drops (no O(N) stats recompute per write). The hub-degree chain-walk pathology is now observable via logs. nexus-core lib 2370 passed; clippy clean. Limitation: commit-granularity batching for CALL IN TRANSACTIONS is not implemented (single transaction); the production tens-of-minutes soak was not reproduced in-session (load-dependent), but the infinite loop is the highest-confidence match and the O(N) stats cost is removed. Per-write refresh_executor (RecordStore mmap clone) remains a known per-write cost not addressed here.
