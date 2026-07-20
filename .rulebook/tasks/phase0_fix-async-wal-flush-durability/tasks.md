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
- [ ] 1.1 Write a failing unit test on `AsyncWalWriter`: submit an entry via
  `append()`, immediately call `flush()`, and — using a test hook or a slowed-down
  writer thread (e.g. inject a sleep before `flush_batch` via a test-only config
  knob, or race-detect via a channel probe) — assert that `flush()` can return
  `Ok(())` before the entry is actually durable on disk (read the underlying WAL
  file directly and confirm the frame is not yet present at the moment `flush()`
  returns). Confirm it fails today (i.e. demonstrates the gap) — a timing-dependent
  test is acceptable here only as a documented, best-effort demonstration; §1.2 is
  the deterministic proof
- [ ] 1.2 Add a deterministic version: instrument or wrap `writer_thread` (or use a
  test-only variant) to artificially delay `flush_batch`'s execution of
  `WalCommand::Flush` by a controllable gate (e.g. a channel the test holds closed),
  call `flush()` on the main thread, assert it returns before the gate is released,
  then release the gate and assert the data becomes durable afterward — this
  isolates the "enqueue vs. complete" gap without relying on scheduler timing
- [ ] 1.3 Confirm by inspection that `Engine::flush_async_wal`
  (`engine/mod.rs:822-827`) has exactly one production caller today
  (`recover_external_ids_from_wal`, `engine/mod.rs:695`) and that its queue is
  empty at that call site (called once at startup before any writer submits
  entries) — this documents why the bug is latent today, not a repro requirement

## 2. Design the completion handshake
- [ ] 2.1 Decide the handshake shape: extend `WalCommand::Flush` to carry a
  `std::sync::mpsc::Sender<Result<()>>` (or a `oneshot`-style single-use channel) so
  `writer_thread` can signal completion back to the specific `flush()` call that
  requested it. Record the choice and why (mpsc single-send vs. a dedicated oneshot
  crate) in the proposal
- [ ] 2.2 Define the ordering guarantee precisely: `flush()` must block until
  `flush_batch` has processed every entry that was successfully `append()`-ed
  *before* `flush()` was called (not just "some future flush eventually happens") —
  state this as the exact contract the implementation must satisfy, matching what
  the existing doc comment already promises
- [ ] 2.3 Decide how `Shutdown` interacts with a pending `Flush` handshake: if
  `shutdown()` races a caller blocked in `flush()`, the blocked caller must still
  receive a completion signal (the final drain/flush in `writer_thread`'s shutdown
  path, `async_wal.rs:362-383`, already flushes remaining entries — verify the
  handshake is honored on that path too, not only the normal `WalCommand::Flush`
  branch)

## 3. Implement the fix
- [ ] 3.1 Change `WalCommand::Flush` to carry the completion channel designed in
  §2.1; update `flush()` (`async_wal.rs:251-260`) to create the channel, send the
  command, and block on `recv()` before returning, translating a disconnected
  channel or a propagated error into the appropriate `Err`
- [ ] 3.2 Update `writer_thread`'s `WalCommand::Flush` arm (`async_wal.rs:333-339`)
  to run `flush_batch`, capture its actual outcome, and signal it through the
  handshake channel before continuing the loop
- [ ] 3.3 Thread `flush_batch`'s real result (success, or the exhausted-retries /
  emergency-save failure case at `async_wal.rs:489-501`) through the handshake
  instead of the caller only ever seeing `Ok(())` regardless of what happened
  inside the batch flush
- [ ] 3.4 Handle the §2.3 shutdown-race case: ensure a `flush()` call concurrent
  with `shutdown()` either completes via the normal handshake or is not left
  blocked forever if the writer thread exits first
- [ ] 3.5 Make the §1.1 and §1.2 tests pass: `flush()` now only returns after the
  data is durable, verified by reading the WAL file immediately after the call
  returns with no gate/delay needed

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/wal-mvcc.md` with the async WAL flush contract:
  `flush()` blocks until the fsync backing all previously-submitted entries has
  completed; add a CHANGELOG entry
- [ ] 4.2 Tests: `flush()` blocks until durable (§1.2 made deterministic and kept as
  a permanent regression test); `flush()` propagates a real error when `flush_batch`
  fails after retries; `recover_external_ids_from_wal`'s existing startup call still
  passes end-to-end with the blocking `flush()`; a `flush()` call concurrent with
  `shutdown()` does not hang
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-wal-torn-tail-recovery` — the recovery path that calls
  `flush_async_wal()` before re-reading the WAL file depends on this barrier being
  real, not just enqueued
- `phase0_fix-wal-durability-gaps` — other durability gaps (unreplayable emergency
  batch, missing directory fsync, no checkpoint/truncate) in the same WAL subsystem
