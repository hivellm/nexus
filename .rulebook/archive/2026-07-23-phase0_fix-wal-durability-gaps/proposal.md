# Proposal: phase0_fix-wal-durability-gaps

**Priority: MEDIUM — three independent durability-hardening gaps in the WAL
subsystem: an emergency fallback batch that is written in a format nothing ever
reads back, missing directory fsync after first-file creation, and a WAL that is
never checkpointed or truncated in production.** Found during a durability/crash-
recovery audit; not previously reported.

## Why

### Gap #4 — emergency batch unreplayable

When the normal WAL flush exhausts its 3 retries, `flush_batch` calls
`Self::emergency_save_batch(batch)` (`crates/nexus-core/src/wal/async_wal.rs:500`)
to avoid losing the entries outright:

```rust
fn emergency_save_batch(batch: &[WalEntry]) {
    let backup_path = format!("data/wal-emergency-{}.log", chrono::Utc::now().timestamp());

    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&backup_path)
    {
        Ok(mut file) => {
            for entry in batch {
                if let Ok(data) = bincode::serialize(entry) {
                    let _ = file.write_all(&(data.len() as u32).to_le_bytes());
                    let _ = file.write_all(&data);
                }
            }
            ...
```

(`async_wal.rs:503-526`). Two independent defects here:

1. **Format mismatch.** The frame layout written is `[len:4][bincode]` — no magic
   byte, no algo byte, no CRC. `Wal::recover()` (`writer.rs:444-617`) only
   understands the real frame formats (`[magic|type][algo?][len][payload][crc]`,
   v1/v2/v3) and never opens a file matching `wal-emergency-*.log` in the first
   place. There is no boot-time scan for this filename pattern anywhere in the
   codebase. The log message even says `"CRITICAL: Failed to flush WAL batch after
   {} retries. {} entries lost!"` (`async_wal.rs:493-497`) — the code's own
   diagnostic already admits the entries are lost, and the emergency save does not
   change that outcome because nothing ever reads the file back.
2. **Wrong location.** `format!("data/wal-emergency-{ts}.log")` is relative to the
   process's current working directory, not the configured WAL/data directory
   (`Wal::new`'s `path` parameter, `writer.rs:132-163`). A server started from a
   different CWD than its data dir silently writes the "emergency" file somewhere
   outside the data directory entirely, making manual recovery harder even for an
   operator who knows to look for it.

### Gap #5 — no directory fsync after file creation

`Wal::new` creates the WAL's parent directory and the file itself but never fsyncs
the parent directory:

```rust
pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
    let path = path.as_ref().to_path_buf();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .read(true).write(true).create(true).truncate(false)
        .open(&path)?;
    ...
```

(`writer.rs:132-146`). `Wal::with_cipher` fsyncs the **file** after writing its
page header (`file.sync_all()?`, `writer.rs:201`) but likewise never fsyncs the
**directory** entry for that file. The record stores have the identical gap:
`RecordStore::new` creates `nodes.store`/`rels.store`, zero-fills and `sync_all()`s
each file (`crates/nexus-core/src/storage/record_store.rs:66-112`, e.g.
`nodes_file.sync_all()?` at `:98`), but never opens and fsyncs `path` (the parent
directory) itself.

POSIX does not guarantee a newly-created file's directory entry is durable until
the containing directory is fsynced — an `fsync()` on the file only guarantees the
file's *data*, not the *directory metadata* that makes the file discoverable after
a crash. The exact crash window: a crash between "file created + its own data
fsynced" and "parent directory fsynced" can, on a POSIX filesystem after a power
loss or OS crash (behavior varies by filesystem, but the guarantee genuinely does
not exist without the directory fsync), leave the file's data durable on disk but
its directory entry not — the file may not exist, may be zero-length, or may point
at stale data when the filesystem is recovered, even though the create+write+fsync
sequence the process performed looked complete.

### Gap #6 — WAL never checkpointed/truncated in production

`Wal::checkpoint` and `Wal::truncate` exist and are implemented correctly:

```rust
pub fn checkpoint(&mut self, epoch: u64) -> Result<()> {
    let entry = WalEntry::Checkpoint { epoch };
    self.append(&entry)?;
    self.flush()?;
    self.stats.checkpoints += 1;
    self.stats.entries_since_checkpoint = 0;
    Ok(())
}
```

(`writer.rs:422-431`), and `truncate`/`truncate_to` (`writer.rs:639-670`). But
their only callers in the entire codebase are `tests/integration.rs:537,540`
(`wal.checkpoint(epoch).unwrap(); ... wal.truncate().unwrap();`) — there is no
production call site. Consequences:

- Every mutation appends to the WAL forever; nothing ever calls `checkpoint` or
  `truncate` on a live server, so the file only grows.
- `Wal::health_check` rejects the WAL once it exceeds 1 GB (`writer.rs:794`), so a
  sufficiently long-lived instance eventually starts **failing all WAL writes** —
  a hard availability cliff with no operator-facing remediation short of manual
  intervention.
- `recover()` re-reads the **entire** WAL history on every boot
  (`writer.rs:444-617` scans from `frames_start` to EOF unconditionally), so
  startup cost grows monotonically with the instance's total lifetime writes,
  not with the amount of recent, un-checkpointed state.
- It also makes `phase0_fix-wal-torn-tail-recovery`'s poison-frame failure mode
  *permanent by construction*: since nothing ever truncates the file, a torn tail
  from any crash is never cleared by a routine checkpoint — the file just keeps
  growing past it (before that task's fix) or stays torn indefinitely with no
  compaction opportunity (even after that task's fix, an unbounded WAL is strictly
  worse for recovery time and disk usage than one that is periodically compacted).

## What Changes

- **#4**: write the emergency batch using the *real* WAL frame format (through the
  same `Wal`/`append` machinery, or an equivalent encoder that produces
  `[magic|type][algo][len][payload][crc]` frames) into the *actual configured data
  directory* rather than a CWD-relative path; add a boot-time scan that discovers
  `wal-emergency-*` files in the data directory and replays/merges them into
  recovery before or alongside the main WAL. If a genuinely unreplayable format is
  ever kept for any reason, that path must instead surface a hard error at
  emergency-save time so the operator halts and intervenes, rather than logging
  "entries lost" and continuing to serve traffic on an inconsistent WAL.
- **#5**: after creating a new file (WAL file in `Wal::new`/`with_cipher`;
  `nodes.store`/`rels.store` in `RecordStore::new`), open the file's parent
  directory and call `sync_all()` (or the platform equivalent) on it so the
  directory entry is durable before the constructor returns.
- **#6**: wire a periodic checkpoint into the production write path — e.g. driven
  by entry count, elapsed time, or WAL size, analogous to `flush_batch`'s existing
  age/size triggers — that: fsyncs the record stores and catalog, appends a
  `WalEntry::Checkpoint` marker, and truncates the WAL prefix up to that point. On
  recovery, resume replay from the last checkpoint rather than from the start of
  the file.

## Impact

- Affected specs: `docs/specs/wal-mvcc.md` (emergency-save format and replay
  contract, checkpoint/truncate production contract, directory-durability guarantee
  for file creation)
- Affected code: `crates/nexus-core/src/wal/async_wal.rs` (`emergency_save_batch`
  `:503-526`), `crates/nexus-core/src/wal/writer.rs` (`Wal::new` `:132-163`,
  `with_cipher` `:177-201`, `checkpoint`/`truncate`/`truncate_to` `:422-431,
  639-670`, `health_check` `:794`), `crates/nexus-core/src/storage/record_store.rs`
  (`RecordStore::new` `:66-112`)
- Breaking change: NO — all three changes are additive hardening; on-disk frame
  format for normal (non-emergency) WAL writes is unchanged, and checkpointing
  truncates only entries already durably reflected in storage
- User benefit: entries that hit the emergency-save fallback are actually
  recoverable instead of silently lost; a crash immediately after first-run
  directory/file creation cannot leave a store the process believes exists but the
  filesystem does not durably record; long-lived instances no longer face an
  unbounded WAL growth cliff, unbounded startup recovery cost, or a permanently
  un-compactable torn-tail poison frame
- Related: `phase0_fix-wal-torn-tail-recovery` (gap #6 is what turns that task's
  single-boot poison into a permanent one), `phase0_fix-async-wal-flush-durability`
  (same WAL subsystem, the flush barrier that gap #4's emergency path falls back
  from when retries are exhausted)
