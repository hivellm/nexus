# Proposal: phase1_unsafe-wal-stats-race

## Why

`AsyncWalWriter::append`, `flush`, and `writer_thread` all mutate stats via
`unsafe { &mut *(Arc::as_ptr(&self.stats) as *mut AsyncWalStats) }`. The Arc is
shared between the caller thread and the background writer thread, so two
threads can reach a `&mut` to the same struct concurrently — textbook UB in
safe Rust terms and not guarded by any `// SAFETY:` comment (explicit
requirement from `.claude/rules/rust.md`). The fields being mutated are
counters (`entries_submitted`, `current_queue_depth`, etc.) that have natural
`AtomicU64` implementations, so the unsafe dance is replaceable.

## What Changes

- Redefine `AsyncWalStats` so every mutably-updated field is `AtomicU64`
  (or `AtomicUsize`).
- Replace every `unsafe { ... }` block that obtains `&mut AsyncWalStats`
  with `self.stats.<field>.fetch_add(1, Ordering::Relaxed)` (or
  `Acquire/Release` where ordering matters).
- `stats()` accessor returns a plain-value snapshot by loading each atomic.
- Remove the `unsafe` pointer manipulation entirely.

## Impact

- Affected specs: none
- Affected code:
  - `nexus-core/src/wal/async_wal.rs:146, 166, 223, 225` (and the struct
    definition around line 45)
- Breaking change: NO (`AsyncWalStats`'s public shape stays compatible —
  callers only read the snapshot)
- User benefit: removes a real UB vector; makes the code honest about
  what it does; closes the gap with `rust.md` rule #5
