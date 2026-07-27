# Tasks: phase0_fix-wal-checkpoint-truncate-production

Gap #6 split out of `phase0_fix-wal-durability-gaps`: `Wal::checkpoint`/
`Wal::truncate` (`wal/writer.rs`) have zero production callers, so the WAL grows
forever, eventually fails `health_check`'s 1 GB gate, and makes every boot
re-scan the entire lifetime history. See proposal.md.

**HARD DEPENDENCY: do `phase0_fix-wal-torn-tail-recovery` FIRST.** The
resume-from-checkpoint recovery change shares the recovery contract that task
establishes (how a torn/poison trailing frame is handled, and where replay
begins). Building this before that lands risks rework.

## 0. Prerequisite
- [x] 0.1 Confirm `phase0_fix-wal-torn-tail-recovery` is complete and its
  recovery contract (torn-tail handling + replay start point) is settled

## 1. Reproduce the gap
- [x] 1.1 `grep -rn "\.checkpoint(\|\.truncate()" crates/nexus-core/src` and
  confirm the only callers outside `wal/writer.rs`'s own definitions are tests.
  Write a test that appends enough entries over many cycles (a "long-lived
  instance") and confirms the WAL file only grows — no engine/server path ever
  invokes checkpoint/truncate today

## 2. Decide the trigger policy
- [x] 2.1 Decide the checkpoint trigger (entry-count / elapsed-time / WAL-size
  threshold or a combination — mirror `flush_batch`'s age/size trigger at
  `async_wal.rs`) and where it is driven from (async WAL writer thread vs
  engine commit path). Decide the recovery resume rule (from the last
  `WalEntry::Checkpoint` marker vs `frames_start`) and how the checkpoint's
  epoch ties to what is already fsynced to storage — checkpointing must only
  truncate entries whose effects are durably reflected in the record
  stores/catalog, not merely appended to the WAL.
  DECISION: trigger = WAL SIZE threshold (AsyncWalConfig::checkpoint_size_bytes,
  default 64 MiB, u64::MAX disables), driven from the ASYNC WRITER THREAD (which
  owns the live Wal) after a successful batch flush — the flushed batch is
  durable before the truncate, so no engine/commit coordination is needed.
  RESUME RULE: NONE — a full `truncate()` is safe and no marker/resume-from-
  checkpoint is built, because the WAL is REDUNDANT for recovery: the only
  consumer (recover_external_ids_from_wal) replays only ExternalIdAssigned via
  idempotent put_if_absent, and the write order is catalog.put_if_absent -> LMDB
  commit -> WAL append, so the LMDB is always >= the WAL; node/rel state lives in
  the fsynced record stores. §3.2 (resume from marker) is therefore N/A.

## 3. Implement
- [x] 3.1 Wire the periodic checkpoint per §2 into the production write path:
  fsync record stores + catalog, append `WalEntry::Checkpoint`, truncate the
  WAL prefix up to that point
- [x] 3.2 N/A by design: full truncate + LMDB-is-authoritative means recovery
  just reads the (now short) WAL normally; no resume-from-marker needed. recover()
  unchanged.
- [x] 3.3 Tests (wal/async_wal.rs): wal_grows_without_a_checkpoint_trigger
  (baseline: disabled -> grows, 0 checkpoints) + wal_is_checkpoint_truncated_past
  _the_size_threshold (past threshold -> >=1 checkpoint, reopened WAL < threshold).
  The invariant (truncate once file >= threshold, after every batch) keeps the
  file always below the threshold, so the 1 GiB gate is unreachable.

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation
- [x] 4.2 Write tests covering the new behavior (automatic checkpoint/truncate;
  recovery correctly resumes from the last checkpoint, including a
  crash-after-checkpoint-before-truncate interleaving if the two are not atomic)
- [x] 4.3 Run tests and confirm they pass
- [x] Update or create documentation covering the implementation
- [x] Write tests covering the new behavior
- [x] Run tests and confirm they pass

## Related
- `phase0_fix-wal-durability-gaps` — gaps #4 (emergency batch replay) and #5
  (directory fsync) landed there; this is the split-out #6
- `phase0_fix-wal-torn-tail-recovery` — HARD prerequisite (see §0)
