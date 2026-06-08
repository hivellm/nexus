# Proposal: phase6_fix-sustained-write-busyloop

Source: GitHub issue #12 (https://github.com/hivellm/nexus/issues/12)

## Why
Under a sustained write replay (graph backfill: a long stream of
`MERGE (n:Label {id:X}) SET ...` + edge MERGEs, indexes present and
engaging), Nexus 2.3.1 enters a 100% CPU internal busy-loop and becomes
fully unresponsive:
- Zero `Executing Cypher` log lines for 90s+ (only the health-check loop).
- No `slow query still running` warnings (not a single stuck query the
  monitor can see).
- A trivial `RETURN 1` times out at 20s.
- CPU pinned at ~100%; only a `docker restart` clears it (recovery ~6s on
  the clean ~19k-node graph).
The stall appears after replaying ~18k nodes / tens of minutes of
continuous writes (~1-2 writes/sec). Indexes were created before the
replay and engaging (no `UnindexedPropertyAccess`), so this is NOT the
unindexed-scan path. Likely index maintenance / a background task / lock
or memory pressure under sustained write load — no single query is
implicated.

## What Changes
- Investigation first (no fix without a confirmed root cause — this is a
  diagnostic-led task):
  - Add/curate telemetry to surface what the server is doing when no query
    is running: background-task / index-maintenance / WAL / flush / GC
    activity, lock-wait/contention counters, and a thread/stack snapshot
    hook (or a `GET /admin/...` introspection endpoint) so a busy-loop with
    no active query is observable.
  - Reproduce under sustained write load (drive ~tens of thousands of
    `MERGE`+`SET` and edge MERGEs with indexes engaging) until the
    100% CPU / unresponsive state is hit; capture where the CPU is spinning
    (hot loop, lock spin, retry loop, unbounded background queue).
  - Identify the busy-loop root cause (candidate areas: index maintenance
    on every write, WAL/flush/async-writer loop, transaction/epoch
    retry, relationship-index rebuild, a poll/spin without backoff).
- Fix the identified hot loop so sustained writes drain instead of pinning
  the core and wedging the server; add a regression/soak guard.

## Impact
- Affected specs: ops / observability, storage / write path, index maintenance
- Affected code: TBD pending investigation — likely
  `crates/nexus-core/src/engine/` write path, `index/` maintenance, `wal/`,
  `transaction/`, and/or `nexus-server` admin/telemetry surface
- Breaking change: NO (reliability + observability fix)
- User benefit: sustained backfills/write bursts no longer wedge the
  server at 100% CPU; operators can see what a no-query-running server is
  doing.

## Notes
- Likely coupled to #13 (UNWIND writes silently dropped): the one-
  statement-per-write churn caused by #13 is the sustained-load pattern
  that exposes this stall. Fixing #13 reduces the trigger; this task fixes
  the underlying busy-loop and adds the telemetry to diagnose it.
- Repro is load-dependent; the reporter can capture heap / thread dump /
  verbose logs if pointed at the knobs — define those knobs as part of the
  telemetry work.
