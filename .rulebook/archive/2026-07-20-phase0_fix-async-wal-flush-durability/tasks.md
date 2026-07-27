# Tasks: phase0_fix-async-wal-flush-durability

`AsyncWalWriter::flush()` (`crates/nexus-core/src/wal/async_wal.rs:248-260`)
documents itself as ensuring "all previously submitted entries are written and
synced to disk," but its body only sends `WalCommand::Flush` on an mpsc channel and
returns `Ok(())` as soon as the send succeeds — before the background
`writer_thread` has dequeued the command, run `flush_batch`, or fsynced anything.
The trigger is any caller that treats the `Ok(())` return as proof of durability.

Order matters: prove the race with a test (§1) before adding the completion
handshake, because the handshake design (§2) must be verified against an actual
observed race, not a hypothesized one; only after the handshake exists can the
retry/emergency-save error path be threaded through it (§3) and the caller
(`recover_external_ids_from_wal`) re-verified to still work under the blocking
semantics (§4).

## 1. Reproduce the race first
- [x] 1.1 Timing-dependent demonstration of the gap (best-effort). Done — the
  deterministic §1.2 gate test is the permanent proof.
- [x] 1.2 Deterministic gate test:
  `flush_blocks_until_writer_thread_signals_completion` — a `#[cfg(test)]`
  `flush_gate` on `AsyncWalConfig` holds the writer at the `Flush` arm; the test
  asserts `flush()` has NOT returned while gated, releases the gate, then asserts
  the frame is durable and `flush()` returns. Fails pre-fix (flush() returned
  immediately). Kept as a permanent regression test.
- [x] 1.3 Confirmed: `flush_async_wal`'s only production caller is
  `recover_external_ids_from_wal` at startup with an empty queue — why the bug
  is latent today, not a repro requirement.

## 2. Design the completion handshake
- [x] 2.1 Handshake shape recorded in proposal "## Decision (§2.1)": a fresh
  single-use `std::sync::mpsc::channel::<Result<()>>()` per `flush()` call (no new
  crate; no cross-call correlation). `WalCommand::Flush` carries the `Sender`.
- [x] 2.2 Ordering contract: `flush()` blocks until `flush_batch` has flushed the
  batch containing every entry appended before `flush()` was called. Guaranteed by
  crossbeam FIFO — the `Flush` command is dequeued after all prior `Append`s, so
  the in-memory `batch` already holds them when the `Flush` arm flushes.
- [x] 2.3 Shutdown/Flush interaction: the writer's final drain loop handles a
  raced `Flush(tx)` (runs the flush, acks via `tx`). Plus a hang-proof guard — see
  §3.4.

## 3. Implement the fix
- [x] 3.1 `WalCommand::Flush(mpsc::Sender<Result<()>>)`; `flush()` creates the
  channel, sends, and blocks for the ack (see §3.4 for the non-hanging wait).
- [x] 3.2 The `writer_thread` `Flush` arm runs `flush_batch`, clears the batch, and
  `ack_tx.send(result)` (non-panicking) before continuing.
- [x] 3.3 `flush_batch` now returns the real `Result` (faithful `last_error` after
  exhausted retries / emergency save); it travels back through the handshake, so
  `flush()` returns it verbatim instead of a blind `Ok(())`.
- [x] 3.4 Shutdown-race made hang-proof (hardened after code review found a
  crossbeam trap: a `Flush` buffered after the final drain but before the writer
  exits is neither processed nor dropped, so plain `recv()` would hang forever).
  The writer sets a shared `writer_exited: AtomicBool` as its last act (via a
  `Drop` guard, fires on panic too); `flush()` waits with `recv_timeout` and, on a
  timeout where `writer_exited` is set, returns `Err` instead of blocking. A
  cleanly-dropped `ack_tx` surfaces as `Disconnected → Err`. Verified by
  `flush_concurrent_with_shutdown_does_not_hang`.
- [x] 3.5 Both §1 tests pass — `flush()` returns only after the data is durable.

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation:
  `docs/specs/wal-mvcc.md` async WAL flush-barrier contract; CHANGELOG [3.0.0]
  `### Fixed — phase0_fix-async-wal-flush-durability`. Done.
- [x] 4.2 Write tests covering the new behavior: `flush()` blocks until durable
  (gate test, permanent regression); `flush()` propagates a real error after
  retries exhausted; `flush()` concurrent with `shutdown()` does not hang; existing
  startup `recover_external_ids_from_wal` path still passes. Done — all in
  `async_wal.rs` `#[cfg(test)]`.
- [x] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test -p nexus-core`). Done: WAL async tests 9 passed/0 failed
  (no hang, 0.71s), full nexus-core suite green (20 groups, 0 failed); fmt clean;
  clippy exit 0. Code-reviewed; the hang finding was fixed before commit.

## Related
- `phase0_fix-wal-torn-tail-recovery` — the recovery path that calls
  `flush_async_wal()` before re-reading the WAL file depends on this barrier being
  real, not just enqueued
- `phase0_fix-wal-durability-gaps` — other durability gaps (unreplayable emergency
  batch, missing directory fsync, no checkpoint/truncate) in the same WAL subsystem
