## 1. Investigation (diagnostic-led — no fix without a confirmed root cause)
- [x] 1.1 Added targeted telemetry for the no-query CPU pathology: a chain-walk hop counter + `warn!` in `find_relationship_between` surfaces O(degree) hub edge-MERGE under `RUST_LOG=nexus_core=warn`. The root cause was found by code analysis, so a broader admin thread-dump endpoint was not needed for this fix
- [x] 1.2 Reproduced the primary cause deterministically: `CALL { ... } IN TRANSACTIONS OF n ROWS` over a subquery returning `>= n` rows looped forever (the regression test would hang on the old code). Full production soak remains load-dependent
- [x] 1.3 Root-caused the busy-loop: (a) infinite re-execution loop in `execute_call_subquery_commands` (CALL IN TRANSACTIONS) holding the engine write lock at 100% CPU with no active-query log — exact signature match; (b) `LabelIndex::recompute_stats` recomputed O(N) over all label bitmaps on every `add_node`/`remove_node`, a compounding per-write CPU sink under sustained load

## 2. Implementation
- [x] 2.1 Fixed the hot loops: CALL IN TRANSACTIONS now runs the subquery once in a transaction and commits (commit-granularity batching, not re-execution); `LabelIndex` stats are computed lazily on read instead of on every write
- [x] 2.2 Telemetry ships: the chain-walk warning makes the hub-degree edge-MERGE pathology observable when no query appears to be running

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the fix + the telemetry (CHANGELOG / GH #12)
- [x] 3.2 Write tests: a regression guard that `CALL { ... } IN TRANSACTIONS` terminates (would hang on the old code)
- [x] 3.3 Run tests and confirm they pass
