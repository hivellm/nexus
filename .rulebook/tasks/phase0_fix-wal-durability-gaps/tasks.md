# Tasks: phase0_fix-wal-durability-gaps

Three independent WAL-subsystem durability-hardening gaps: (#4) the emergency
fallback batch (`crates/nexus-core/src/wal/async_wal.rs:503-526`) is written in a
frame format `Wal::recover()` never parses and to a CWD-relative path outside the
data dir, so entries it claims to "save" are permanently unreadable; (#5) new WAL
and record-store files (`wal/writer.rs:132-163`, `storage/record_store.rs:66-112`)
are fsynced themselves but their parent directory is never fsynced, so a crash
right after first-run creation can leave the file undiscoverable; (#6)
`Wal::checkpoint`/`Wal::truncate` (`writer.rs:422-431,639-670`) have zero
production callers (only `tests/integration.rs:537,540`), so the WAL grows forever,
eventually fails `health_check`'s 1 GB gate (`writer.rs:794`), and makes every
future boot's recovery scan re-read the entire lifetime history.

Order matters: prove each gap independently first (§1) — they are unrelated code
paths, so each needs its own repro, but proving all three before fixing any avoids
re-deriving the fix design mid-implementation. Decide the fix approach for each
(§2) before implementing (§3), since #4's format choice and #6's checkpoint trigger
policy are design decisions the proposal deliberately left open. Fix order within
§3 follows the gaps' discovery order (#4, #5, #6) — they do not depend on each
other, so this is not a load-bearing sequence, only a consistent one. Tail (§4)
runs last and covers all three.

## 1. Reproduce and scope each gap
- [ ] 1.1 Gap #4: write a test that forces `flush_batch`'s retry loop to exhaust
  (e.g. inject a WAL write failure via a test double or a read-only/locked target
  path) so `emergency_save_batch` (`async_wal.rs:503-526`) runs. Confirm today: (a)
  the resulting `wal-emergency-*.log` file uses `[len:4][bincode]` framing with no
  magic/algo/CRC bytes, (b) calling `Wal::new(&data_dir).recover()` against the
  data directory afterward does NOT return the emergency-saved entries, and (c) the
  file lands at a CWD-relative `data/wal-emergency-*.log`, not inside the
  configured data directory, when the process CWD differs from the data dir
- [ ] 1.2 Gap #5: confirm by inspection that `Wal::new` (`writer.rs:132-163`),
  `Wal::with_cipher` (`writer.rs:177-201`), and `RecordStore::new`
  (`record_store.rs:66-112`) each call `create_dir_all` and/or `sync_all()` on the
  file but never open and fsync `path.parent()`. Where the platform/test harness
  allows it, add a test that opens the parent directory handle after each
  constructor and confirms no fsync was issued on it (e.g. via an instrumented
  wrapper, or documenting the gap as inspection-verified if directory-fsync
  observation isn't feasible in the test environment — state which in the PR)
- [ ] 1.3 Gap #6: `grep -rn "\.checkpoint(\|\.truncate()" crates/nexus-core/src` and
  confirm the only call sites outside `wal/writer.rs`'s own definitions are
  `tests/integration.rs:537,540`. Write a test that appends enough entries to grow
  a `Wal` past a small threshold, runs for a simulated "long-lived instance" (many
  append cycles), and confirms the file size only grows — `checkpoint`/`truncate`
  are never invoked by any engine/server code path today

## 2. Decide the fix approach for each gap
- [ ] 2.1 Gap #4: decide whether the emergency batch is written through the real
  `Wal`/frame-encoding machinery (reusing `Wal::append`'s frame format directly
  against a `Wal` instance pointed at the configured data directory) or via a
  standalone encoder producing byte-identical frames; record the choice and why.
  Decide the boot-time discovery pattern for `wal-emergency-*` files (glob on
  startup before/alongside main WAL recovery) and how their entries merge into the
  replayed entry stream (ordering relative to the main WAL's entries matters —
  state the ordering rule)
- [ ] 2.2 Gap #5: decide the directory-fsync helper's shape (a small shared
  function called from `Wal::new`, `Wal::with_cipher`, and `RecordStore::new`) and
  confirm it degrades gracefully (or documents its limits) on platforms/filesystems
  where directory fsync is a no-op or unsupported, so the fix does not turn a
  missing-durability-guarantee gap into a hard startup failure on such platforms
- [ ] 2.3 Gap #6: decide the checkpoint trigger policy (entry count threshold,
  elapsed time, WAL size threshold, or a combination — mirror the existing
  `flush_batch` age/size trigger pattern at `async_wal.rs:348-350` for consistency)
  and decide where it is driven from (the async WAL writer thread, or the engine's
  commit path). Decide the recovery-side change: resuming replay from the last
  `WalEntry::Checkpoint` marker instead of from `frames_start` every boot, and how
  the checkpoint marker's epoch ties to what has already been fsynced to storage
  (checkpointing must only truncate entries whose effects are durably reflected in
  the record stores/catalog, not merely appended to the WAL)

## 3. Implement the fixes
- [ ] 3.1 (Gap #4) Reroute `emergency_save_batch` (`async_wal.rs:503-526`) to write
  real WAL frames into the actual configured data directory per §2.1; add the
  boot-time scan/replay for `wal-emergency-*` files per §2.1's merge ordering
- [ ] 3.2 (Gap #4) Make the §1.1 test pass: after the fix, entries that hit the
  emergency-save path are present in `recover()`'s output following a normal
  restart-and-recover sequence
- [ ] 3.3 (Gap #5) Add the directory-fsync helper from §2.2; call it from
  `Wal::new` (`writer.rs:132-163`) and `Wal::with_cipher` (`writer.rs:177-201`)
  after the WAL file is created, and from `RecordStore::new`
  (`record_store.rs:66-112`) after `nodes.store`/`rels.store` are created
- [ ] 3.4 (Gap #5) Make the §1.2 verification pass (test or documented inspection
  confirmation, per what §1.2 established as feasible)
- [ ] 3.5 (Gap #6) Wire the periodic checkpoint per §2.3's trigger policy into the
  production write path; update recovery to resume from the last checkpoint marker
  instead of scanning from `frames_start` unconditionally
- [ ] 3.6 (Gap #6) Make the §1.3 test pass: a long-running append sequence now
  triggers `checkpoint`/`truncate` automatically and the WAL file size stays
  bounded relative to un-checkpointed entries rather than growing with total
  lifetime writes; confirm `health_check`'s 1 GB gate (`writer.rs:794`) is no
  longer reachable under normal sustained operation

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/wal-mvcc.md` with: the emergency-save format and its
  boot-time replay contract, the directory-fsync guarantee for newly created WAL
  and record-store files, and the checkpoint/truncate production trigger policy and
  recovery-from-checkpoint contract. Add a CHANGELOG entry covering all three gaps
- [ ] 4.2 Tests: emergency-saved entries survive a full restart-and-recover cycle;
  directory-fsync helper is called from all three constructors (or the inspection
  confirmation from §1.2/3.4 is recorded); a long-lived `Wal` triggers automatic
  checkpoint/truncate and recovery correctly resumes from the last checkpoint,
  including a crash-after-checkpoint-before-truncate interleaving if the checkpoint
  and truncate are not atomic
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-wal-torn-tail-recovery` — gap #6 (never checkpointed/truncated) is
  what turns that task's torn-tail poison-frame failure into a permanent one; both
  should land with a shared understanding of the recovery contract
- `phase0_fix-async-wal-flush-durability` — same WAL subsystem; gap #4's emergency
  path is the fallback taken when the normal `flush_batch` retries (which that
  task's blocking-flush contract also depends on) are exhausted
