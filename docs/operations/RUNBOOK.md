# Nexus operations runbook

This runbook collects the operator-facing procedures for diagnosing
and recovering a misbehaving Nexus instance. It is meant to be the
first thing on call rotation reads when paged.

## Diagnosing a wedged server

A "wedged" Nexus is one where the writer thread is saturated and
`/cypher` requests start timing out. The reference incident is
`cortex-nexus` 2026-05-04: a Cortex ingestion pipeline ran
`MERGE (n:Artifact { natural_key: $v })` against an unindexed
`Artifact.natural_key` for 33 hours, every MERGE doing a full label
scan, single-writer queue saturated, every `count(*)` and `/stats`
timing out at 5–10 s.

Three signals to combine:

1. **Slow-query log**

   Background tick task in `crates/nexus-server/src/lib.rs` polls the
   active-query map every `NEXUS_SLOW_QUERY_TICK_MS` (default
   `1000`). For any query whose `elapsed >= NEXUS_SLOW_QUERY_THRESHOLD_MS`
   (default `1000`) it emits a WARN log to
   `target = "nexus_server::slow_query"`:

   ```
   WARN nexus_server::slow_query{query_id=query-42 connection_id=conn-7 elapsed_ms=12000}
        slow query still running: MERGE (n:Artifact { natural_key: ... }) RETURN n
   ```

   Per-query rate-limit: first warn on threshold crossing, then once
   every `NEXUS_SLOW_QUERY_REPEAT_SECS` (default `30`) for as long
   as the query is still in the running set. Setting
   `NEXUS_SLOW_QUERY_THRESHOLD_MS=0` disables the tick entirely.

   Filter the log:

   ```bash
   docker logs cortex-nexus 2>&1 | grep slow_query
   ```

2. **`GET /admin/queries`**

   When `/cypher` is the wedged surface, the slow-query log is the
   only signal you can read with confidence. As a complementary
   check, hit `/admin/queries` directly — it only reads the in-memory
   tracker map (`ConnectionTracker::get_queries`) and never touches
   the executor, so it stays responsive even when the writer is
   blocked.

   ```bash
   curl -s http://localhost:15474/admin/queries | jq '.entries[] | select(.status == "running")'
   ```

   Response shape (`schema_version: 1`):

   ```json
   {
     "total": 3,
     "running": 1,
     "entries": [
       {
         "query_id": "query-42",
         "connection_id": "conn-7",
         "query": "MERGE (n:Artifact { natural_key: 'sha256:...' }) RETURN n",
         "started_at_secs": 1714770000,
         "elapsed_ms": 12000,
         "status": "running"
       }
     ],
     "schema_version": 1
   }
   ```

   Entries are sorted by `elapsed_ms` descending so the longest-running
   query is first. The `query` field is truncated at 8 KiB at the
   wire boundary.

3. **`SHOW QUERIES` / `TERMINATE QUERY`**

   Cypher introspection over the `/cypher` endpoint. Same data
   source as `/admin/queries`. Use this when you can still hit
   `/cypher` (e.g. read-only paths still respond) and want the
   results in the same format as the rest of your tooling:

   ```cypher
   SHOW QUERIES;
   TERMINATE QUERY 'query-42';
   ```

   `TERMINATE QUERY` flips the entry's `cancelled` flag. The
   executor cooperatively checks this flag — long-running operators
   surface a `Query was cancelled` error on the next yield point.

## Why a panic during execution doesn't leak a "running" entry

Every Cypher request that reaches the HTTP handler holds a
`RegisteredQueryGuard` (`crates/nexus-core/src/performance/connection_tracking.rs`).
The guard's `Drop` impl calls `complete_query`, so:

- Normal return → guard drops on function exit, entry transitions
  `is_running: true` → `false`.
- Early return via `?` → guard still in scope, drops on the way out.
- Panic during execution → guard drops during stack unwinding.

Without the guard (the previous design), a panic inside the
executor would leave the entry in `is_running: true` indefinitely.
The slow-query log would then bark forever and `SHOW QUERIES` would
list a phantom entry until the cleanup tick reaped it 10 minutes
later.

## Tuning knobs

| Variable | Default | Effect |
|---|---|---|
| `NEXUS_SLOW_QUERY_TICK_MS` | `1000` | Tick interval. `0` disables. |
| `NEXUS_SLOW_QUERY_THRESHOLD_MS` | `1000` | Threshold above which a query is "slow". `0` disables the tick. |
| `NEXUS_SLOW_QUERY_REPEAT_SECS` | `30` | Per-query log throttle window. |
| `NEXUS_PLANNER_WARN_INTERVAL_SECS` | `60` | Window for the related `Nexus.Performance.UnindexedPropertyAccess` planner WARN log (see [`docs/performance/PERFORMANCE.md`](../performance/PERFORMANCE.md)). |

## Related diagnostics

- The `Nexus.Performance.UnindexedPropertyAccess` notification
  surfaces in the `/cypher` response envelope and the server log
  when MERGE/MATCH selects nodes by `(label, property)` without a
  covering index. Pair the slow-query log with the notification to
  confirm whether a wedged query is an index-pathology issue.
- `GET /stats` reports throughput, page-cache hit rate, and writer
  queue depth. When the writer is saturated, `/stats` itself can
  time out — `/admin/queries` is the more reliable triage entry
  point in that state.
