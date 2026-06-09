## 1. Investigation
- [ ] 1.1 Confirm the bounded(1000) channel + blocking `sender.send()` (async_wal.rs:170,215) and that the caller holds the engine write lock while blocked (crud.rs:1046)
- [ ] 1.2 Confirm the synchronous WAL append+flush path exists and is durable as a fallback; note the channel_buffer_size vs max_queue_depth mismatch

## 2. Implementation
- [ ] 2.1 Use `try_send` on the hot path; on `Full`, fall back to the synchronous WAL append+flush (never block the engine lock)
- [ ] 2.2 Align channel capacity with `max_queue_depth` (or document the difference)

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation (CHANGELOG / GH #19)
- [ ] 3.2 Write tests: a burst exceeding the channel capacity still makes progress (no stall) and remains durable (WAL replay recovers all entries)
- [ ] 3.3 Run tests and confirm they pass
