# Proposal: phase0_fix-async-wal-flush-durability

**Priority: MEDIUM-HIGH — `AsyncWalWriter::flush()` is documented as a durability
barrier but returns as soon as the flush command is enqueued, before the batch is
actually fsynced, making the barrier illusory for any caller that relies on it.**
Found during a durability/crash-recovery audit by two independent audits; not
previously reported.

## Why

`AsyncWalWriter::flush()` documents itself as a synchronous durability barrier:

```rust
/// Force flush all pending entries
///
/// This ensures all previously submitted entries are written and synced to disk.
pub fn flush(&self) -> Result<()> {
    use std::sync::atomic::Ordering::Relaxed;
    self.stats.force_flushes.fetch_add(1, Relaxed);

    self.sender
        .send(WalCommand::Flush)
        .map_err(|_| Error::wal("Failed to send flush command - channel closed"))?;

    Ok(())
}
```

(`crates/nexus-core/src/wal/async_wal.rs:248-260`). The body only sends a
`WalCommand::Flush` down an mpsc channel and returns `Ok(())` the instant the send
succeeds — there is no acknowledgement, no oneshot channel, no wait for the
background thread to act on the command. The actual fsync happens later, on the
separate `writer_thread`, when it dequeues `WalCommand::Flush` and calls
`Self::flush_batch(&mut wal, &batch, &stats, config)` (`async_wal.rs:333-339`),
which internally calls `wal.flush()` (the underlying `Wal`'s real fsync,
`async_wal.rs:441-467`).

So `flush()`'s return says nothing about whether the fsync has happened —
only that the *request* to fsync was handed off. A caller that calls `flush()` and
then, believing the doc comment, acknowledges a commit as durable is acknowledging
data that may still be sitting unflushed in the writer thread's channel/batch,
recoverable only after `max_batch_age` or a queue drain later (or lost entirely on
a crash before the writer thread processes the command).

`Engine::flush_async_wal` is a thin pass-through with the same false guarantee:

```rust
pub fn flush_async_wal(&mut self) -> Result<()> {
    if let Some(ref writer) = self.async_wal_writer {
        writer.flush()?;
    }
    Ok(())
}
```

(`crates/nexus-core/src/engine/mod.rs:822-827`).

Today the only production caller is `recover_external_ids_from_wal`
(`engine/mod.rs:695`, `self.flush_async_wal()?` before `Wal::new(&wal_path)?.recover()`
re-reads the file), called once at startup before any writer submits entries — the
queue is empty, so the race is benign *today*. But the write-commit path
(`crates/nexus-core/src/engine/transactions.rs:70-71`) flushes storage but never
calls the WAL flush barrier at all: `CreateNode`/`ExternalIdAssigned` frames written
via the async WAL during a transaction remain queued (not yet fsynced) at the moment
the commit is acknowledged to the caller. The barrier being non-blocking means it is
unusable as-is for that gap to be closed correctly later — any future caller that
adds a `flush_async_wal()` call to the commit path to fix that would still not get a
true durability guarantee, because the fix under review here has not happened yet.

## What Changes

- Make `AsyncWalWriter::flush()` block until the background writer thread has
  actually executed the flush (i.e. the fsync inside `flush_batch` → `wal.flush()`
  has returned), not merely enqueued the request. Add a completion handshake to
  `WalCommand::Flush` — e.g. carry a `std::sync::mpsc::Sender<Result<()>>` (or a
  oneshot) that the `writer_thread` signals after `flush_batch` completes for that
  specific command — and have `flush()` wait on the corresponding receiver before
  returning.
- `flush_batch`'s per-attempt retry/emergency-save logic must propagate its actual
  outcome through that handshake so a caller blocked in `flush()` gets a faithful
  `Result` (success, or an error reflecting exhausted retries), not a blind `Ok(())`.
- No change to `Engine::flush_async_wal`'s signature — it stays a pass-through; its
  guarantee becomes real once the underlying `flush()` blocks correctly.

## Impact

- Affected specs: `docs/specs/wal-mvcc.md` (durability barrier contract for the
  async WAL path)
- Affected code: `crates/nexus-core/src/wal/async_wal.rs` (`flush()` `:248-260`,
  `WalCommand` enum, `writer_thread` `:292-384`, `flush_batch` `:386-501`);
  `crates/nexus-core/src/engine/mod.rs` (`flush_async_wal` `:822-827`, call site
  `:695`)
- Breaking change: NO for the public signature (`flush(&self) -> Result<()>`
  unchanged); behavioral change only — `flush()` now blocks for real, so callers
  that assumed near-instant return under load may observe added latency, which is
  the correct trade for a genuine durability barrier
- User benefit: any current or future caller of `flush_async_wal()`/`flush()` gets
  the guarantee its own doc comment already promises — data submitted before the
  call is verifiably fsynced before the call returns — closing the gap that made the
  barrier unusable as a foundation for a durable commit path
- Related: `phase0_fix-wal-torn-tail-recovery` (the recovery path this barrier
  guards depends on the flush actually completing before `recover()` re-reads the
  file); `phase0_fix-wal-durability-gaps` (other durability gaps in the same WAL
  subsystem)
