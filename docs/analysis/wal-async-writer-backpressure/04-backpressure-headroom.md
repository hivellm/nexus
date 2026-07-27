# 04 — Backpressure headroom: queue dynamics and drain time

## Queue capacity and sizing

The async writer's command channel has capacity determined at creation (`async_wal.rs:246–251`):

```rust
pub fn new(wal: Wal, config: AsyncWalConfig) -> Result<Self> {
    // #19: size the command channel from the configured max queue depth so
    // the `max_queue_depth` knob is real (previously the channel was bound
    // only by `channel_buffer_size`, so `max_queue_depth` merely fed a
    // counter and the writer blocked far earlier than configured).
    let capacity = config.channel_buffer_size.max(config.max_queue_depth);
    let (sender, receiver) = bounded(capacity);
    // ...
}
```

With defaults (`channel_buffer_size = 1000`, `max_queue_depth = 10000`), the channel capacity is `max(1000, 10000) = 10,000`.

This means the async queue can hold up to **10,000 pending `Append` commands** before the sender blocks.

## The append() fallback: try-send → blocking send

`append()` (`async_wal.rs:287–329`) submits entries with a non-blocking-first policy:

```rust
pub fn append(&self, entry: WalEntry) -> Result<()> {
    // ... (update stats)
    
    match self.sender.try_send(WalCommand::Append(entry)) {
        Ok(()) => Ok(()),  // Fast path: queue had space
        Err(TrySendError::Full(cmd)) => {
            self.stats.backpressure_blocks.fetch_add(1, Relaxed);
            tracing::warn!(
                queue_depth = new_depth,
                "WAL async channel full — applying backpressure ..."
            );
            self.sender
                .send(cmd)  // Blocking send: wait for writer to drain
                .map_err(|_| Error::wal("Failed to send WAL command - channel closed"))
        }
        Err(TrySendError::Disconnected(_)) => {
            Err(Error::wal("Failed to send WAL command - channel closed"))
        }
    }
}
```

The flow is:

1. **Fast path** (`try_send`): if the channel has capacity, enqueue immediately and return.
2. **Backpressure path** (`send`): if the channel is at `max_queue_depth`, **block** on the crossbeam channel until the writer thread drains entries (via `recv()` in the writer loop, line 451).
3. Unblock once the writer consumes an entry and the queue drops below capacity.

This backpressure is **intentional and bounded**.

## Backpressure is bounded by one batch-fsync latency

When `append()` blocks on a full queue, it is blocked waiting for the writer thread's `recv_timeout()` (line 450) to consume an entry. The writer consumes entries and flushes them in batches.

Worst-case scenario:

1. Queue is at `max_queue_depth` (10,000 entries).
2. Producer calls `append()` → `try_send()` fails → blocks on `send()`.
3. Writer thread is mid-batch, has not yet called `wal.flush()`.
4. Writer accumulates more entries (up to `max_batch_size`) and then calls `flush_batch`.
5. `flush_batch` calls `wal.flush()` (the fsync), which takes ~12–14 ms at batch size 1000.
6. Once the fsync completes, the writer's `recv_timeout()` loop (line 450) unblocks and calls `recv()` again, consuming the next entry from the channel.

**The blocking duration is bounded by the time from the blocked `send()` call to the next successful `recv()`, which is bounded by one batch-fsync latency (the time for `flush_batch` + the writer's loop iteration time).**

On this hardware, one batch-fsync is ~12–14 ms at batch size 1000, so a single `append()` stall is tens of milliseconds, not unbounded.

## Queue drain time under full-queue worst case

If the queue fills completely (10,000 entries) and the producer is stalled, how long does it take to drain?

At batch size 1000:

```text
Entries to drain: 10,000
Batch size: 1,000 entries
Batches needed: 10 batches
Latency per batch (fsync): ~12–14 ms (median, per 02-measurement.md)
Total drain time: 10 × 12–14 ms ≈ 120–140 ms
```

This is the **worst-case drain time**: a full queue with no new appends (the producer is stalled). In practice:

- The producer stalls only when the queue is full, so the queue stabilizes at ~10k once backpressure begins.
- The writer thread flushes batches continuously, draining the queue.
- After the first batch fsync completes, the queue drops below 10k, the producer's `send()` unblocks, and new entries resume flowing in.
- Equilibrium is re-established: producer and consumer rates balance.

A drain time of ~140 ms is **well under any client timeout** (HTTP defaults are 30+ seconds). Backpressure in the WAL layer is transparent to the application — the producer stalls briefly (tens of milliseconds) and then resumes, keeping the connection alive. This is correct throttling and does not cause connection drops.

## Why no change to queue capacity was needed

The batch-size fix (`max_batch_size: 100 → 1000`) improves throughput (54k → 71k+ entries/s) independent of queue sizing. No changes to queue capacity are needed because:

1. **Backpressure is safe and bounded**: under heavy sustained load (the real LDBC submit rate ~600k rel/s), the queue will fill and remain full (pinned at ~10,000), with backpressure blocks engaging continuously. Each `append()` stall is bounded by one batch-fsync (~12–14 ms), which is transparent to the application.

2. **Drain time is acceptable**: a full 10k queue drains in ~140 ms worst case, well under any client timeout or application deadline. The queue remains stable once filled; the producer and consumer settle into a steady equilibrium where producer throughput matches the durable-fsync rate.

3. **Backpressure prevents memory exhaustion**: the bounded queue capacity ensures the WAL layer cannot grow unbounded, even under produce-rates exceeding the fsync rate. This is correct and intentional.

Therefore, `max_queue_depth` (10,000) and `channel_buffer_size` (1,000) do not require changes. The batch-size fix alone improves sustained throughput; queue saturation under heavy load is a feature, not a bug.

## Conclusion

The async WAL queue is sized conservatively with a 10,000-entry capacity. Under heavy sustained load at batch size 1000 (the real LDBC relationship-submit rate), the queue will fill to capacity and remain full—backpressure is continuously engaged. This is correct behavior: the queue stabilizes, ensuring the producer cannot outpace the consumer by more than the queue capacity, preventing unbounded memory growth.

When the queue fills, each `append()` call stalls for ~12–14 ms (one batch-fsync latency) until the writer consumes an entry. A complete queue drain takes ~140 ms worst case—well under application timeouts. These stalls are transparent to the application and do not cause connection drops.

No changes to `max_queue_depth` or channel sizing are needed. The batch-size fix alone provides the throughput improvement; queue saturation under real bulk load is safe and intentional.
