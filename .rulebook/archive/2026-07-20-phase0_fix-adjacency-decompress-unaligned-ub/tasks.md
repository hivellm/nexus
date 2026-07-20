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
- [x] 1.1 Test `test_decompress_none_from_misaligned_offset` (in the
  `compression.rs` `#[cfg(test)] mod tests`) builds a `None`-compressed buffer
  beginning at an odd offset inside a larger backing `Vec<u8>` (prefix 1 byte,
  slice `[1..]`) — exactly the shape `engine.rs` hands `decompress_none` for an
  odd `list_offset` — and asserts the decode reconstructs the entries.
- [x] 1.2 miri available and used. `cargo +nightly miri test` on a temporary
  reproduction of the OLD `from_raw_parts::<AdjacencyEntry>` cast reported
  `Undefined Behavior: constructing invalid value: encountered an unaligned
  reference (required 8 byte alignment but found 1)`; the NEW decode passes
  miri clean. Objective before/after signal captured; temp repro removed.
- [x] 1.3 Byte layout recorded: little-endian `u64` `rel_id`, `entry_count * 8`
  bytes (length-checked at each site). The replacement decode preserves it and
  the encode side was made explicitly little-endian to match (see §2).

## 2. Replace the unsafe cast with an alignment-agnostic decode
- [x] 2.1 `decompress_none` now uses the shared `decode_adjacency_entries_le`
  (`chunks_exact(8)` + `u64::from_le_bytes`); the `unsafe` /
  `slice::from_raw_parts` block is gone.
- [x] 2.2 Same decode applied to `decompress_lz4`.
- [x] 2.3 Same decode applied to `decompress_zstd`.
- [x] 2.4 Grep of `graph_engine/`: the other `from_raw_parts` sites
  (`compress_none`/`compress_lz4`/`compress_zstd` at 117/225/277, and
  `format.rs` `calculate_checksum` at 150) all cast `AdjacencyEntry`/`Self` →
  `u8` — going to a LESS-strict alignment (u8 = align 1), so NOT the UB. The
  three compress sites were nonetheless switched to explicit
  `encode_adjacency_entries_le` (`to_le_bytes`) so the on-disk encoding is
  byte-order-symmetric with the LE decode (avoids a latent big-endian
  round-trip break); the checksum cast (a byte-sum, byte-order-agnostic) is
  left as-is.
- [x] 2.5 Re-ran the misaligned-offset test on the new code: passes cleanly,
  and `cargo miri test` reports no UB for it.

## 3. Tail (docs + tests — check or waive with tailWaiver)
- [x] 3.1 Update or create documentation covering the implementation — DONE:
  added an "Adjacency-List Encoding" section to `docs/specs/storage-format.md`
  (little-endian `u64` contract, no 8-byte alignment assumption on decode) and a
  CHANGELOG entry under `[3.0.0]`.
- [x] 3.2 Write tests covering the new behavior — DONE: misaligned-offset
  regression test (clean under miri) plus `None`/LZ4/Zstd round-trip tests kept
  in the suite, confirming decoded values are identical after the decode change.
- [x] 3.3 Run tests and confirm they pass — DONE (green): `cargo +nightly fmt
  --all`, `cargo clippy -p nexus-core --all-targets --all-features -- -D
  warnings` (0 warnings), full `cargo +nightly test -p nexus-core` and
  `cargo +nightly test --workspace` — 0 failed.

## Related
- `phase0_fix-storage-oob-panics` — sibling storage-layer bounds/UB hardening
  found in the same audit pass
