# Async WAL Writer Backpressure Ceiling Analysis

This directory documents the async WAL writer's sustained throughput ceiling under bulk ingest workloads, the root cause (conservative batch-size default), the measurement methodology, and the policy fix that improves durable-WAL throughput. The fix is independent of and does NOT resolve the LDBC SF0.1 client-connection abort (which is a separate per-IP rate-limiter issue being fixed separately).

## Quick Summary

The async WAL writer already implements group commit: one fsync per batch, not per entry. The real limiter was the default `max_batch_size = 100`, which set sustained durable throughput to roughly `100 / 1.8ms ≈ 54k entries/s`. Raising `max_batch_size` to 1000 (one policy knob, no mechanism change) lifts sustained throughput to ~71–112k entries/s.

Under the real bulk submit rate in LDBC SF0.1 (~52–68k rows/s, measured to reach ~600k rel/s under the post-catalog-fsync conditions), the async queue stays **full by design** and backpressure remains continuously engaged. This is correct throttling: the writer drains entries as fast as they are durable-flushed (one batch fsync per ~12–14ms), and the queue depth pins at the configured maximum (10,000). A bounded stall (~12–14ms, one batch-fsync latency) on each `append()` is the intended backpressure mechanism, not a failure. Durability is unchanged: still exactly one fsync per batch, recovery unchanged, `flush()` remains a real barrier.

## Files

1. **[01-ceiling-root-cause.md](01-ceiling-root-cause.md)** — Why the 100-entry batch size was the ceiling, why group commit alone does not guarantee headroom, and what changes when a more aggressive bulk ingest exposes the queue.

2. **[02-measurement.md](02-measurement.md)** — The `wal_throughput.rs` benchmark harness, its design rationale (why not criterion), the per-config measurement methodology, and the key insight: sustained throughput follows the `max_batch_size / fsync_latency` formula closely.

3. **[03-fix-and-durability.md](03-fix-and-durability.md)** — The fix (raise `max_batch_size` 100 → 1000), why it changes ONLY policy (batch-size), not mechanism (one fsync per batch still), and verification that durability and recovery are unaffected.

4. **[04-backpressure-headroom.md](04-backpressure-headroom.md)** — Queue capacity analysis: the channel capacity is 10,000, the producer-side stall is bounded by one batch-fsync (~12–14ms), and backpressure is safe and intentional under sustained load. Confirms the batch-size fix provides a throughput ceiling improvement independent of connection failures.

## Context

This analysis documents a real throughput improvement to the WAL layer, discovered during analysis of the LDBC SF0.1 relationship-load failure. After the catalog-fsync fix (documented in `docs/analysis/ingest-relationship-throughput/04_results.md`), the relationship-ingest rate jumped from ~3k to ~50k–70k rel/s locally, and a live server test revealed the real submit rate under sustained bulk load reaches ~600k rel/s. At this rate, the async WAL queue stays full (`queue_depth=10001`) and backpressure is continuously engaged — this is correct behavior, not a failure.

The LDBC SF0.1 load abort (os error 10053, "connection reset by peer") at the identical point across multiple attempts was initially attributed to the WAL ceiling, but root-cause analysis revealed a separate issue: the server's per-IP rate limiter (100 req/min, no loopback exemption) returns HTTP 429 without draining the large request body, causing hyper to RST the keep-alive connection. The WAL batch-size fix improves throughput (54k → 71k+ entries/s measured), but does NOT resolve this connection drop. Both issues are being fixed independently: the WAL batch-size change (throughput improvement, this analysis) and the rate-limiter fix (loopback exemption, drain-body-on-429, configurable limits).
