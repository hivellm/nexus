# Proposal: phase0_fix-wal-checkpoint-truncate-production

**Priority: MEDIUM — the WAL is never checkpointed or truncated in production,
so it grows forever, eventually fails `health_check`'s 1 GB gate (a hard
availability cliff), and makes every boot re-scan the entire lifetime history.**
This is gap #6, split out of `phase0_fix-wal-durability-gaps` (whose #4 and #5
landed) because its recovery-from-checkpoint contract is deliberately sequenced
AFTER `phase0_fix-wal-torn-tail-recovery`.

## Why

`Wal::checkpoint` and `Wal::truncate`/`truncate_to`
(`crates/nexus-core/src/wal/writer.rs`) exist and are implemented correctly, but
their only callers in the entire codebase are the WAL's own tests
(`wal/mod.rs` `test_checkpoint`, and historically `tests/integration.rs`). There
is **no production call site**. Consequences:

- Every mutation appends to the WAL forever; nothing ever calls `checkpoint` or
  `truncate` on a live server, so the file only grows.
- `Wal::health_check` rejects the WAL once it exceeds 1 GB (`writer.rs`), so a
  sufficiently long-lived instance eventually starts **failing all WAL writes** —
  a hard availability cliff with no operator-facing remediation short of manual
  intervention.
- `recover()` re-reads the **entire** WAL history on every boot (it scans from
  `frames_start` to EOF unconditionally), so startup cost grows monotonically
  with the instance's total lifetime writes, not with the amount of recent,
  un-checkpointed state.
- It makes `phase0_fix-wal-torn-tail-recovery`'s poison-frame failure mode
  *permanent by construction*: since nothing ever truncates the file, a torn
  tail from any crash is never cleared by a routine checkpoint.

## What Changes

- Wire a periodic checkpoint into the production write path — driven by entry
  count, elapsed time, or WAL size (mirror `flush_batch`'s existing age/size
  trigger pattern), driven either from the async WAL writer thread or the
  engine's commit path — that: fsyncs the record stores and catalog, appends a
  `WalEntry::Checkpoint` marker, and truncates the WAL prefix up to that point.
- On recovery, resume replay from the last `WalEntry::Checkpoint` marker rather
  than from the start of the file. The checkpoint's epoch must only truncate
  entries whose effects are **durably reflected** in the record stores/catalog,
  never entries merely appended to the WAL.

## Dependency (why this is sequenced after torn-tail-recovery)

The recovery-side change here (resume-from-checkpoint) shares the recovery
contract that `phase0_fix-wal-torn-tail-recovery` establishes: how `recover()`
treats a torn/poison trailing frame, and where replay begins. Implementing
checkpoint-truncate + resume-from-checkpoint BEFORE that task lands risks
building against a recovery contract that then changes. Do
`phase0_fix-wal-torn-tail-recovery` first, then this.

## Impact

- Affected specs: `docs/specs/wal-mvcc.md` (checkpoint/truncate production
  trigger policy and recovery-from-checkpoint contract)
- Affected code: `crates/nexus-core/src/wal/writer.rs`
  (`checkpoint`/`truncate`/`truncate_to`, `health_check`, `recover`),
  `crates/nexus-core/src/wal/async_wal.rs` (trigger, if driven from the writer
  thread), `crates/nexus-core/src/engine/*` (trigger, if driven from commit;
  recovery resume point)
- Breaking change: NO — additive; checkpointing truncates only entries already
  durably reflected in storage
- User benefit: long-lived instances no longer hit an unbounded WAL growth
  cliff, unbounded startup recovery cost, or a permanently un-compactable
  torn-tail poison frame
