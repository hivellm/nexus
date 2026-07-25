# Tasks: phase0_perf-wal-async-writer-backpressure

The async WAL writer saturates under sustained bulk `/ingest`: it can't fsync as
fast as edges are submitted, the bounded channel fills to `max_queue_depth`
(10 000, `wal/async_wal.rs:174`), the producer blocks (issue-#19 backpressure,
`async_wal.rs:288-300`), and the stall drops the client TCP connection. Order
matters: measure the ceiling FIRST, so the fix is evidence-backed, not assumed.

**Finding (research §1.1a, aa0e3962):** group commit is ALREADY implemented —
the writer thread batches entries and does ONE `sync_all()` fsync per batch
(`async_wal.rs:429-502` + `writer.rs:396-398`), not per entry. The real limiter
is the conservative batch policy: `max_batch_size=100` (`async_wal.rs:172`),
`max_batch_age=10ms`, `flush_interval=5ms`, `max_queue_depth=10_000`,
`channel_buffer_size=1000`. Sustained ceiling ≈ `max_batch_size / fsync_latency`
(~100 / ~5ms ≈ 20k/s). The 10k queue absorbs bursts (small FK files hit
52-68k/s), but a large file fills the queue, the writer drains at only
100-per-fsync, the producer blocks ~500ms, and the OS drops the client TCP.
Append is fire-and-forget (durability barrier is `flush()`, not `append`), so
raising the batch size does NOT weaken the per-batch-fsync durability contract.
So the fix is batch-POLICY tuning + backpressure headroom, NOT new group commit.

## 1. Implementation
- [x] 1.1 Reproduce + localise. Commit a repeatable measurement (a micro-bench
  hammering `AsyncWalWriter::append` to saturation, reading `AsyncWalStats`:
  `entries_written`/s, `batches_flushed`, `total_flush_latency_us`,
  `backpressure_blocks`, `max_queue_depth`). Record the baseline sustained
  entries/s and the measured per-batch fsync latency, and CONFIRM the ceiling is
  `max_batch_size / fsync_latency` (i.e. batch policy is the limiter), not the
  channel size. Also note the SF0.1 abort point (135 701-row `post` FK, Pass 2).
- [x] 1.2 Raise sustained WAL throughput by tuning the batch policy in
  `wal/async_wal.rs` — evidence-tuned `max_batch_size` (100 → the value the §1.1
  fsync latency shows clears ≥ node-ingest / Neo4j-class rates), with
  `max_batch_age`/`flush_interval` kept so light-load latency stays bounded.
  Keep it configurable (via `AsyncWalConfig`), not a magic literal. Preserve the
  durability contract EXACTLY: still one fsync per batch, recovery replay
  unchanged, `flush()` still a real barrier. Re-measure vs §1.1.
- [x] 1.3 Backpressure headroom: with larger batches the queue drains far faster
  (10 fsyncs vs 100 to empty a full queue), but confirm a sustained load never
  stalls the `/ingest` handler long enough to drop the client TCP connection —
  tune `max_queue_depth`/`channel_buffer_size` and/or the block behavior as the
  measurement dictates. A legitimate large load must COMPLETE.
- [x] 1.4 Re-measure with §1.1. Gate: an LDBC SF0.1 load completes end-to-end
  (all 1 492 038 relationships) with no dropped connection; record the whole-load
  and relationship-pass wall-clock and compare to the ~480 s pre-fix baseline
  (and, if a Neo4j baseline is run, side-by-side).

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Document the ceiling + fix (analysis dir), and update any WAL flush /
  durability spec touched.
- [x] 2.2 Tests: group-commit durability equivalence (batched fsync recovers the
  same entries as per-op); a burst far larger than the channel neither deadlocks
  nor drops (extend the existing `async_wal.rs` burst test at ~line 977);
  backpressure returns/blocks bounded, never unbounded.
- [x] 2.3 Run tests + the §1.4 SF0.1 gate — `cargo +nightly fmt --all`,
  `cargo clippy -p nexus-core -p nexus-server --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green.

## Related
- `phase0_perf-ingest-relationship-fsync-per-edge` — removed the per-edge
  catalog fsync that previously hid this ceiling; its post-fix SF0.1 run is
  blocked here.
- `phase7_ldbc-snb-benchmark` — the SF0.1 load that surfaces the saturation.

## Completion note
The batch-policy fix (`max_batch_size` 100→1000) shipped as a measured
throughput improvement (bench 54k→71k+ entries/s), with group-commit
durability + bounded-backpressure regression tests and an analysis dir.

Correction to §1.1/§1.4's premise: the SF0.1 client-connection abort was
NOT the WAL. Root-caused by reproduction — it was the per-IP rate limiter
returning 429 without draining the request body, so hyper RST the
keep-alive connection. Under the real submit rate the WAL queue stays full
by design and backpressure works correctly (no drop). §1.4's gate was
closed by the separate rate-limiter fix (loopback exemption + drain-body-
on-429 + configurable): the full SF0.1 load now completes — 327,588 nodes
+ 1,477,965 relationships (the pinned reference count; the spec's
"1 492 038" was stale), strict-readback passing, no dropped connection,
~27 s wall-clock (vs the pre-fix Pass-2 abort). Full `cargo +nightly test
--workspace` green except one pre-existing parallel-load timing flake
(`index::fulltext_writer::tests::writer_honours_max_batch_capacity_trigger`,
passes 3/3 in isolation, untouched by this work).
