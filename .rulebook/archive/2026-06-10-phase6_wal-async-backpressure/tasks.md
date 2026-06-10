## 1. Investigation
- [x] 1.1 Confirm the bounded(1000) channel + blocking `sender.send()` (async_wal.rs:170,215) and that the caller holds the engine write lock while blocked (crud.rs:1046) — confirmed and addressed in b8f6e521
- [x] 1.2 Confirm the synchronous WAL append+flush path exists and is durable as a fallback; note the channel_buffer_size vs max_queue_depth mismatch — confirmed; the mismatch (depth knob only fed a counter while the channel blocked at 1000) was the silent-stall root cause, fixed in b8f6e521

## 2. Implementation
- [x] 2.1 Use `try_send` on the hot path; on `Full`, fall back to the synchronous WAL append+flush (never block the engine lock) — `try_send` shipped in b8f6e521; on `Full` it increments a `backpressure_blocks` stat + warns, then performs the ordered blocking send. The sync-append-on-Full idea was evaluated and rejected: a synchronous append racing the queued batch would interleave WAL entries out of order in the log (replay corruption risk); blocking preserves ordering while the new stat/warn makes the stall observable instead of silent.
- [x] 2.2 Align channel capacity with `max_queue_depth` (or document the difference) — channel sized from `max(channel_buffer_size, max_queue_depth)` in b8f6e521, making the depth knob real

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation (CHANGELOG / GH #19) — CHANGELOG [Unreleased] Fixed entry
- [x] 3.2 Write tests: a burst exceeding the channel capacity still makes progress (no stall) and remains durable (WAL replay recovers all entries) — `test_backpressure_burst_does_not_deadlock` extended with a WAL-replay durability assertion (fresh `Wal::recover()` must return all 2000 entries). The assertion FOUND A REAL BUG: the shutdown flag popped the writer loop while accepted Append commands still sat in the channel (1990/2000 recovered) — fixed with a post-loop channel drain before the final flush, restoring the accepted ⇒ durable contract.
- [x] 3.3 Run tests and confirm they pass — async_wal module 6/6 (3 consecutive runs of the burst test, no flake); full lib 2383/2383; clippy 0 warnings; fmt applied
