# Tasks: phase0_fix-adjacency-decompress-unaligned-ub

Three adjacency-list decompression functions construct a typed
`&[AdjacencyEntry]` from an unaligned `&[u8]` via `slice::from_raw_parts`
(`compression.rs:439-441` none, `:604-606` lz4, `:646-648` zstd) — undefined
behaviour per the function's documented safety contract, triggerable in
production because `decompress_none` runs directly against an mmap
sub-slice whose start offset (`engine.rs:432-447`, `list_offset`) carries no
alignment guarantee.

Order matters: first prove the misalignment is real and reachable with a
targeted (non-UB, safe) test (§1) so the fix has a concrete regression
guard, then replace the unsafe cast with a safe byte-decode uniformly
across all three sites (§2) so no call site is left with the old pattern —
partial fixes would leave the same UB reachable through whichever function
is skipped.

## 1. Reproduce the misalignment risk
- [ ] 1.1 Write a test that builds an adjacency-list byte buffer whose
  entries begin at an odd (non-8-aligned) byte offset within a larger
  backing `Vec<u8>` (e.g. by prefixing 1 extra byte before slicing), and
  calls the current `decompress_none` on that misaligned sub-slice.
  Document (in the test comment, not asserted, since UB cannot be reliably
  asserted) that this is exactly the shape of slice `engine.rs:432-447`
  hands to `decompress_adjacency_list` for an odd `list_offset`
- [ ] 1.2 Confirm via `cargo miri test` (or note in the task if `miri` is
  unavailable in this environment) that the misaligned-offset test in 1.1
  is flagged as UB under the current implementation, giving an objective
  before/after signal independent of "it happened not to crash on x86"
- [ ] 1.3 Record the exact byte layout `decompress_none`/`decompress_lz4`/
  `decompress_zstd` expect today (little-endian `u64` `rel_id`,
  `entry_count * 8` bytes, `compression.rs:428`/`:595`/`:637`) so the
  replacement decode preserves it exactly

## 2. Replace the unsafe cast with an alignment-agnostic decode
- [ ] 2.1 Implement the `chunks_exact(8)` + `u64::from_le_bytes` decode in
  `decompress_none` (`compression.rs:439-441`), removing the `unsafe`
  block and the `slice::from_raw_parts` call entirely
- [ ] 2.2 Apply the same decode to `decompress_lz4` (`compression.rs:604-606`)
- [ ] 2.3 Apply the same decode to `decompress_zstd` (`compression.rs:646-648`)
- [ ] 2.4 Grep `crates/nexus-core/src/storage/graph_engine/` for any other
  `from_raw_parts` cast of `AdjacencyEntry` (or another `#[repr(C)]` type)
  that this task's scope may have missed, and fix or explicitly note as
  out of scope with a reason
- [ ] 2.5 Re-run the 1.1/1.2 misaligned-offset test against the new
  implementation and confirm it now passes cleanly (and, if `miri` was used
  in 1.2, that `cargo miri test` reports no UB for this test)

## 3. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 3.1 Update `docs/specs/storage-format.md` to state explicitly that
  the adjacency-list decompression path must not assume 8-byte alignment of
  `list_offset`, and document the byte-decode contract; add a CHANGELOG
  entry
- [ ] 3.2 Tests: misaligned-offset regression test from §1/§2 kept in the
  suite; existing adjacency-list compression round-trip tests (none/lz4/zstd)
  still pass unchanged, confirming output values are identical before and
  after the decode change
- [ ] 3.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-storage-oob-panics` — sibling storage-layer bounds/UB hardening
  found in the same audit pass
