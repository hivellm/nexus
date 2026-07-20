# Tasks: phase0_fix-wal-torn-tail-recovery

`Wal::recover()`'s v1/v2 plaintext body reads use bare `?` on `read_exact`
(`crates/nexus-core/src/wal/writer.rs:502-514,528-540`) and a CRC mismatch is always
a hard error (`writer.rs:599-604`), unlike the v3/encrypted path which correctly
truncates a torn trailing frame (`writer.rs:495-499`). A crash mid-append — normal
residue given appends are un-fsynced until the async batch flush
(`wal/async_wal.rs:306-307`) — triggers this path. The caller
(`engine/mod.rs:701-707`) discards the ENTIRE parsed prefix on any `Err`, and
`recover()` never truncates the torn frame, so the poison is permanent across every
future boot.

Order matters: prove the failure mode with a test (§1) before touching `recover()`,
because the fix must be verified against the exact byte-level truncation the test
produces, not a hypothesized one; decide the "last frame in file" boundary condition
(§2) before implementing (§3), since the CRC-mismatch fix depends on knowing whether
a bad frame is trailing or mid-file; only then update the caller and tail (§4).

## 1. Reproduce the loss first
- [ ] 1.1 Write a failing unit/integration test on `Wal`: append several valid
  entries, flush, then truncate the file mid-frame (simulating a crash during the
  next append) so the last frame is missing bytes. Call `recover()` and assert it
  currently returns `Err` and loses the valid prefix (confirm today's behavior
  before changing code)
- [ ] 1.2 Add a CRC-mismatch variant: append valid entries, flush, then corrupt the
  last frame's payload bytes in place (same length, different content) so
  `read_exact` succeeds but the CRC check at `writer.rs:599-604` fails. Confirm
  `recover()` returns `Err` today and the valid prefix is lost
  the same way
- [ ] 1.3 Add a "poison persists across reboots" test: after 1.1's truncated file,
  call `recover()` a second time without any fix applied and confirm the file is
  byte-identical afterward (no truncation happened) — this proves the permanence
  claim, not just the one-boot loss

## 2. Decide the trailing-frame boundary condition
- [ ] 2.1 Define precisely how to detect "this CRC-mismatched frame is the last one
  in the file" vs. mid-file corruption: either (a) the frame's declared
  `payload_len` plus header/CRC size would read past EOF, or (b) after this frame
  there are zero remaining bytes. Record the chosen condition and why in the
  proposal — a CRC mismatch with more frames after it must remain a hard error
- [ ] 2.2 Confirm the `TruncatedTrailing` semantics used by `decode_v3_frame`
  (`writer.rs:495-499`, `:703-731`) for the exact same boundary question, and reuse
  the same reasoning/terminology for the v1/v2 fix so all three frame formats agree
  on what counts as a truncated tail

## 3. Implement the fix
- [ ] 3.1 In the v2 body-read arm (`writer.rs:502-514`), wrap each `read_exact`
  (`type_buf`, `len_buf`, `payload`, `crc_buf`) so `io::ErrorKind::UnexpectedEof`
  calls `self.truncate_to(file_offset)?` (offset of this frame's start, matching
  the pattern at `writer.rs:475-483`) and `break`s instead of propagating `Err`
- [ ] 3.2 Apply the identical fix to the v1 body-read arm (`writer.rs:528-540`)
- [ ] 3.3 Change the CRC-mismatch branch (`writer.rs:599-604`) to check the §2.1
  boundary condition: if this frame is trailing, `truncate_to(file_offset)` and
  `break` (return the prefix); otherwise keep the existing hard `Err`
- [ ] 3.4 Make the §1.1 and §1.2 tests pass (assert `recover()` returns `Ok` with
  the valid prefix, and the file is truncated to the last valid frame boundary);
  make §1.3 pass by asserting a second `recover()` call after the fix returns the
  same prefix with no further truncation (idempotent — the poison is gone for good)

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/wal-mvcc.md` with the recovery contract: a torn
  trailing frame (EOF mid-read, or CRC mismatch on the last frame) truncates and
  returns the valid prefix; a CRC mismatch on a non-trailing frame remains a hard
  error. Add a CHANGELOG entry
- [ ] 4.2 Tests: torn-tail EOF recovers the prefix and truncates (v1 and v2 framing
  both covered); torn-tail CRC mismatch recovers the prefix and truncates;
  mid-file CRC mismatch (not trailing) still hard-errors; a second recovery after
  the fix is a no-op (file unchanged, same entries returned)
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-wal-durability-gaps` — WAL is never checkpointed/truncated in
  production (#6), which is what lets a single torn-tail boot become a permanent
  poison in the first place
- `phase0_fix-async-wal-flush-durability` — the async flush barrier this recovery
  path depends on (`recover_external_ids_from_wal` calls `flush_async_wal()` first,
  `engine/mod.rs:695`) is itself not a real completion barrier
