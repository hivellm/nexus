# 01 — The backpressure ceiling: batch-size policy, not fsync mechanism

## The mechanism: group commit (one fsync per batch)

The async WAL writer (`crates/nexus-core/src/wal/async_wal.rs`) is **already a batched writer**. Its background thread accumulates entries in a `Vec<WalEntry>` and flushes them with exactly **one fsync per batch**, not per entry:

- Line 470–475: when `batch.len() >= max_batch_size`, the thread calls `flush_batch`.
- Line 651–652: `flush_batch` calls `wal.flush()` (the real fsync, `writer.rs:396`) **once** after appending all entries to the WAL file.
- Line 659–668: the batch is counted as flushed, and stats are updated atomically.

This means:

```text
sustained_throughput ≈ max_batch_size / per_batch_fsync_latency
```

On the dev hardware, a single batch fsync takes ~1.8–37 ms (disk variance is wide), so at the default `max_batch_size = 100`, the sustained ceiling sits at roughly:

```text
100 / 1.8 ms ≈ 54,000 entries/s
```

## Why the default 100 was a bottleneck

The default value (`async_wal.rs:191`) was conservative: it prioritised latency-to-durability for small workloads (partial batches flush after ~10ms via `max_batch_age`, line 192) over throughput headroom for sustained bulk ingest.

When the relationship-loader submits entries at ~52–68k rows/s (the throughput after the catalog-fsync fix), a 100-entry batch reaches capacity and flushes in ~1.8ms, then the thread immediately starts the next batch. But:

1. The async queue is bounded at 10,000 entries (`async_wal.rs:193`, `max_queue_depth`).
2. When entries are enqueued faster than they drain (54k/s appended vs ≤54k/s drained), the queue fills.
3. Once the queue reaches `max_queue_depth`, `append()` blocks on a full channel (`async_wal.rs:287–329`, `try_send` → fallback to blocking `send`), stalling the producer thread.
4. The producer stall propagates up the call stack into the ingest handler and (eventually) timeouts the client TCP connection.

This is not a correctness issue — the WAL is still durable (entries accepted into `append()` reach disk via the fsync) — but a **throughput ceiling** below the submit rate.

## Why this remained a real bottleneck (even under the SF0.1 load test)

The catalog-fsync fix (`docs/analysis/ingest-relationship-throughput/02_root_cause.md`) correctly identified and removed a per-relationship fsync, raising relationship-ingest from ~3k to ~50k rel/s in benchmarks. The end-to-end SF0.1 loader (with the catalog fix, before the WAL batch-size fix) stalled at the `post` file with the async queue pinned at `queue_depth=10001` and `backpressure_blocks` counters accumulating.

However, **the connection drop itself is NOT caused by the WAL backpressure**. Root-cause analysis of the live load (with both the catalog fix and the WAL batch-size fix already deployed) revealed that the loader completes 120+ requests successfully under continuous backpressure (queue pinned at 10001 throughout), returning HTTP 200 for every request. The connection is only reset once a *separate* rate-limiter exhausts its per-IP quota (100 req/min), returns HTTP 429 without draining the large request body, and causes hyper to RST the keep-alive connection.

The WAL backpressure is still **real and limits throughput**, but it does not cause client disconnections when the rate limiter is not involved. Under the measured real submit rate (~600k rel/s or ~52–68k rows/s per request file), the async queue stays full by design: the writer drains at ~71k entries/s (at batch size 1000), and the producer feeds entries faster, so the queue stabilizes at the configured maximum. This is correct throttling.

## The Fix: adjust the policy, not the mechanism

Raising `max_batch_size` to 1000 (a **policy knob**) changes nothing about the fsync mechanism. It still does exactly one fsync per batch. But it changes throughput:

```text
1000 / 12–14 ms ≈ 71,000–83,000 entries/s (median run)
```

This is a real throughput improvement. Under light or moderate load, the queue no longer saturates, and backpressure is rare. Under heavy sustained load (e.g., the LDBC relationship loader), the queue will still fill and backpressure will engage, but at a higher equilibrium throughput: the writer can keep up with the real submit rate (~600k rel/s) over sustained periods without dropping entries.

The fsync latency per entry does increase slightly (from 1.8 ms / 100 ≈ 18 µs/entry to ~12 ms / 1000 ≈ 12 µs/entry), but amortisation and group commit ensure it remains small. More importantly, **every entry still reaches the disk via exactly one fsync**, so durability is unchanged — this is not a compromise.

**Note:** under real bulk load, the queue **will remain full** (at `max_queue_depth = 10,000`) and backpressure **will continue to engage** with every `append()` call blocking briefly. This is correct and intentional — it ensures entries drain at exactly the rate they are durable-flushed, preventing unbounded queue growth and memory exhaustion.
