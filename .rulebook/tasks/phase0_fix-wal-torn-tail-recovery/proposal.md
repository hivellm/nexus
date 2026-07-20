# Proposal: phase0_fix-wal-torn-tail-recovery

**Priority: HIGH — a normal crash-residue partial WAL frame makes `recover()` discard
every entry it parsed, including the valid prefix before the torn frame, and the
discard repeats on every future boot because the torn frame is never truncated.**
Found during a durability/crash-recovery audit; not previously reported.

## Why

WAL appends are un-fsynced until the async batch flush
(`crates/nexus-core/src/wal/async_wal.rs:306-307` — the writer thread only flushes
on batch-size/age/interval triggers), so a crash mid-append is expected to leave a
partial trailing frame on disk. `Wal::recover()`
(`crates/nexus-core/src/wal/writer.rs:444-617`) has three parallel body-read paths for
the three frame formats, and only two of them handle that residue correctly.

The v3/encrypted path decodes via `decode_v3_frame`, and a truncated trailing frame
there correctly stops and truncates instead of erroring:

```rust
V3FrameOutcome::TruncatedTrailing => {
    self.truncate_to(file_offset)?;
    break;
}
```

(`writer.rs:495-498`). The `algo_buf` read that precedes it does the same on EOF
(`writer.rs:475-483`). But the v2 plaintext body reads that follow use bare `?`,
which propagates `io::ErrorKind::UnexpectedEof` as a hard `Err` instead of truncating:

```rust
let mut type_buf = [0u8; 1];
self.file.read_exact(&mut type_buf)?;          // writer.rs:503
let mut len_buf = [0u8; 4];
self.file.read_exact(&mut len_buf)?;            // writer.rs:506
let payload_len = u32::from_le_bytes(len_buf) as usize;
let mut payload = vec![0u8; payload_len];
self.file.read_exact(&mut payload)?;            // writer.rs:510
let mut crc_buf = [0u8; 4];
self.file.read_exact(&mut crc_buf)?;            // writer.rs:513
```

The v1 path is the same shape (`writer.rs:531-540`). And a CRC mismatch — the
expected outcome when a payload is torn mid-write so its bytes don't match a CRC
computed over a shorter/garbled buffer — is unconditionally a hard error for both
v1 and v2, with no check for whether the bad frame is the last one in the file:

```rust
if stored_crc != computed_crc {
    return Err(Error::wal(format!(
        "CRC mismatch at offset {} (algo={:?}): expected {:x}, got {:x}",
        file_offset, algo, stored_crc, computed_crc
    )));                                          // writer.rs:599-604
}
```

The caller, `Engine::recover_external_ids_from_wal`
(`crates/nexus-core/src/engine/mod.rs:691-707`), treats any `Err` from `recover()` as
"give up entirely":

```rust
let entries = match replay_wal.recover() {
    Ok(e) => e,
    Err(e) => {
        tracing::warn!("external-id WAL recovery: could not read WAL: {e}");
        return Ok(());
    }
};
```

(`engine/mod.rs:701-707`). Because `entries` is a local `Vec` built up by the loop in
`recover()` (`writer.rs:445,610`) and only returned on `Ok`, an `Err` at frame N
discards frames `0..N` too, not just the torn one.

Critically, `recover()` never calls `truncate_to` on this path, so the crash residue
stays on disk. The next append happens via `SeekFrom::End` (append semantics), i.e.
**after** the torn frame, so the corrupt bytes are never overwritten. Exact crash
window: any crash between "async WAL writer starts writing frame N's bytes" and
"frame N's bytes are fully on disk" leaves this residue; from that point on, **every
subsequent boot** re-hits the same offset, re-fails the same read/CRC check, and
re-discards the same (growing) valid prefix, permanently — this is not a one-boot
hiccup, it poisons the WAL until the file is manually repaired.

## What Changes

- In the v1 and v2 body-read arms of `Wal::recover()` (`writer.rs:502-514,
  528-540`), catch `io::ErrorKind::UnexpectedEof` on each `read_exact` the same way
  the `algo_buf` read already does (`writer.rs:475-483`): call `self.truncate_to`
  with the offset of the start of the current frame and `break` out of the loop,
  returning the successfully-parsed prefix instead of erroring.
- Treat a CRC mismatch (`writer.rs:599-604`) as `TruncatedTrailing` **only when the
  bad frame is the last one in the file** (i.e. reading past it hits EOF, or its
  computed `frame_len` would exceed the file's length) — mirroring the v3
  `TruncatedTrailing` semantics. A CRC mismatch on a frame that is followed by more
  bytes is genuine corruption in the middle of the file and must remain a hard
  error; only the trailing case is ambiguous with ordinary crash residue.
- No caller-visible signature change: `recover()` keeps returning `Result<Vec<WalEntry>>`;
  the fix only changes which cases return `Ok(prefix)` vs `Err`.

## Impact

- Affected specs: `docs/specs/wal-mvcc.md` (recovery contract — what counts as a
  valid crash-truncated tail vs. corruption)
- Affected code: `crates/nexus-core/src/wal/writer.rs` (`recover()` v1/v2 body reads
  `:502-514`, `:528-540`, CRC check `:599-604`); `crates/nexus-core/src/engine/mod.rs`
  (`recover_external_ids_from_wal` `:701-707`, unaffected by this fix but is the
  caller that currently amplifies the loss to "discard everything")
- Breaking change: NO — recovery becomes strictly more permissive (fewer spurious
  full-discards); on-disk frame format is unchanged
- User benefit: a crash mid-WAL-append no longer permanently and silently disables
  external-id crash recovery; the valid prefix before the torn frame survives, and
  the torn residue is truncated so it does not poison every future boot
- Related: `phase0_fix-wal-durability-gaps` (WAL is never checkpointed/truncated in
  production, which is what turns a single torn boot into a permanent one)
