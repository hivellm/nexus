# Proposal: phase0_perf-wal-async-writer-backpressure

**Priority: HIGH (performance). The WAL async writer is the NEXT ceiling on bulk
relationship ingest, now that the per-edge catalog fsync is gone
(`phase0_perf-ingest-relationship-fsync-per-edge`). Under sustained bulk
`/ingest` it saturates, applies blocking backpressure, and the stall is long
enough that the client TCP connection is dropped — aborting a full LDBC SF0.1
load.**

## Why

With relationship ingest no longer catalog-fsync-bound, an end-to-end LDBC SF0.1
load now saturates the async WAL writer instead. Observed reproducibly (two
runs, identical failure point):

- Node pass and early Pass-2 foreign-key files load at ~52 000–68 000 rows/s
  (~17–22× the pre-fix ~3 100 rel/s baseline) — the fast path works.
- The load then aborts at the 135 701-row `post` foreign-key file in Pass 2:
  `WAL async channel full — applying backpressure … queue_depth=10001`
  (`crates/nexus-core/src/wal/async_wal.rs`, the `max_queue_depth: 10_000`
  cap at ~line 174; backpressure block at ~lines 288–300, tagged "issue #19").
- The submitting thread blocks on the bounded command channel long enough that
  the OS tears down the client HTTP/TCP connection (`os error 10053` / `10054`).
  The server itself stays healthy — no crash, `/health` OK.

Root shape: the async writer's fsync throughput is lower than the rate a bulk
`/ingest` submits WAL entries. Issue #19 already sized the channel from
`max_queue_depth` and made backpressure *block rather than drop* — but blocking
the request handler that long still drops the client connection. The real lever
is the per-op fsync cost on the writer side.

## What Changes

Evidence-first (mirror the analysis discipline of the fsync task —
`docs/analysis/`):

1. Measure the WAL writer's fsync throughput ceiling and confirm it (not the
   channel size) is the limiter under bulk ingest.
2. Reduce per-op fsync cost: **group commit** / batched fsync — coalesce many
   queued WAL entries into ONE `fsync` per flush instead of paying a sync per
   op / blocking the producer per op. Keep the durability contract intact.
3. Ensure backpressure never stalls the `/ingest` handler long enough to drop
   the client connection (bounded wait + a clear slow-down signal, or a larger
   drain-ahead margin), so a legitimate large load completes.

## Impact

- Affected specs: none expected (internal durability-path throughput change).
- Affected code: `crates/nexus-core/src/wal/async_wal.rs` (flush/group-commit +
  backpressure policy); possibly the `/ingest` write path and the WAL flush
  trigger.
- Breaking change: NO — same on-disk WAL format, same crash-recovery guarantee;
  only the flush *cadence* (per-op → grouped) and backpressure behavior change.
- User benefit: bulk relationship ingest (LDBC and any large load) stops being
  WAL-fsync-bound and survives without dropped connections; unblocks a complete
  post-fix SF0.1 end-to-end number.
- Related: `phase0_perf-ingest-relationship-fsync-per-edge` (removed the
  catalog fsync that hid this ceiling), `phase7_ldbc-snb-benchmark` (the load
  that surfaces it).
