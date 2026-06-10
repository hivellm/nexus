# Proposal: phase6_wal-async-backpressure

Source: GitHub issue #19 (https://github.com/hivellm/nexus/issues/19)

## Why
The async WAL writer uses a `bounded(1000)` crossbeam channel
(`crates/nexus-core/src/wal/async_wal.rs:170`) and `append()` calls a
blocking `sender.send()` (async_wal.rs:215). Under a sustained write burst
faster than the background fsync rate, the 1000-slot buffer fills and the
main engine write thread blocks on send — while holding the engine write
lock — so all queries stall behind it (a hard throughput ceiling / stall
risk, not just latency). The `max_queue_depth: 10_000` config field only
drives a soft monitoring counter, not the channel size, so the two are
inconsistent and misleading.

## What Changes
- Replace the blocking `sender.send()` on the hot path with `try_send`; on
  `Full`, fall back to the existing synchronous WAL append + flush so the
  writer always makes progress instead of blocking the engine lock.
- Align the actual channel capacity with `max_queue_depth` (or document the
  distinction) so the configured value reflects reality.
- Confirm durability is preserved on the fallback path (crossbeam send
  blocks rather than drops today, so this is reliability, not a data-loss
  fix — keep it that way).

## Impact
- Affected specs: WAL / durability, write back-pressure
- Affected code: `crates/nexus-core/src/wal/async_wal.rs`, the
  `write_wal_async` caller (`crates/nexus-core/src/engine/crud.rs:1046`)
- Breaking change: NO
- User benefit: sustained write bursts no longer stall the server on a full
  WAL channel; consistent, honest queue-depth config.

## Notes
- Audit finding #6.
