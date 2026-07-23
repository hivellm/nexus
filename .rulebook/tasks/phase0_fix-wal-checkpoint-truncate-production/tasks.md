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
- [ ] 0.1 Confirm `phase0_fix-wal-torn-tail-recovery` is complete and its
  recovery contract (torn-tail handling + replay start point) is settled

## 1. Reproduce the gap
- [ ] 1.1 `grep -rn "\.checkpoint(\|\.truncate()" crates/nexus-core/src` and
  confirm the only callers outside `wal/writer.rs`'s own definitions are tests.
  Write a test that appends enough entries over many cycles (a "long-lived
  instance") and confirms the WAL file only grows — no engine/server path ever
  invokes checkpoint/truncate today

## 2. Decide the trigger policy
- [ ] 2.1 Decide the checkpoint trigger (entry-count / elapsed-time / WAL-size
  threshold or a combination — mirror `flush_batch`'s age/size trigger at
  `async_wal.rs`) and where it is driven from (async WAL writer thread vs
  engine commit path). Decide the recovery resume rule (from the last
  `WalEntry::Checkpoint` marker vs `frames_start`) and how the checkpoint's
  epoch ties to what is already fsynced to storage — checkpointing must only
  truncate entries whose effects are durably reflected in the record
  stores/catalog, not merely appended to the WAL

## 3. Implement
- [ ] 3.1 Wire the periodic checkpoint per §2 into the production write path:
  fsync record stores + catalog, append `WalEntry::Checkpoint`, truncate the
  WAL prefix up to that point
- [ ] 3.2 Update recovery to resume from the last checkpoint marker instead of
  scanning from `frames_start` unconditionally
- [ ] 3.3 Make the §1.1 test pass: a long-running append sequence now triggers
  checkpoint/truncate automatically; the WAL size stays bounded relative to
  un-checkpointed entries; `health_check`'s 1 GB gate is no longer reachable
  under normal sustained operation

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation
- [ ] 4.2 Write tests covering the new behavior (automatic checkpoint/truncate;
  recovery correctly resumes from the last checkpoint, including a
  crash-after-checkpoint-before-truncate interleaving if the two are not atomic)
- [ ] 4.3 Run tests and confirm they pass

## Related
- `phase0_fix-wal-durability-gaps` — gaps #4 (emergency batch replay) and #5
  (directory fsync) landed there; this is the split-out #6
- `phase0_fix-wal-torn-tail-recovery` — HARD prerequisite (see §0)
