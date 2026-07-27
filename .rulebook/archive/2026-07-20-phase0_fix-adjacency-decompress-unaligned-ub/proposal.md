# Proposal: phase0_fix-adjacency-decompress-unaligned-ub

**Priority: CRITICAL — every adjacency-list decompression reinterprets an
unaligned byte slice as `&[AdjacencyEntry]` via `slice::from_raw_parts`, which
is undefined behaviour independent of platform or observed symptoms.** Found
during a storage-layer audit; not previously reported.

## Why

`AdjacencyEntry` is declared `#[repr(C)]` with a single `u64` field —
alignment 8 (`crates/nexus-core/src/storage/graph_engine/format.rs:164-169`):

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdjacencyEntry {
    /// Relationship ID
    pub rel_id: u64,
}
```

All three decompression paths reinterpret a `&[u8]` (alignment 1, no
guaranteed 8-byte alignment) as `&[AdjacencyEntry]` via
`std::slice::from_raw_parts`:

- `decompress_none` (`crates/nexus-core/src/storage/graph_engine/compression.rs:439-441`):
  ```rust
  let entries = unsafe {
      std::slice::from_raw_parts(compressed.as_ptr() as *const AdjacencyEntry, entry_count)
  };
  ```
- `decompress_lz4` (`compression.rs:604-606`) — same construct over a
  freshly-decoded `Vec<u8>`.
- `decompress_zstd` (`compression.rs:646-648`) — same construct over a
  freshly-decoded `Vec<u8>`.

`slice::from_raw_parts::<T>` requires the pointer to be aligned to
`align_of::<T>()` **as a documented safety precondition**; passing an
unaligned pointer is immediate undefined behaviour the moment the slice
value is constructed — it does not require the data to ever be
dereferenced through the misaligned typed pointer, and it is a separate
defect from the `to_vec()` copy that follows. The only guard present at each
call site is a **length** check (`compressed.len() != expected_size`,
`compression.rs:430` and the `expected_bytes` checks at `:596`/`:638`);
alignment of `compressed.as_ptr()` is never checked or enforced anywhere on
this path.

`decompress_none` operates directly on the caller-supplied `compressed: &[u8]`
slice, which is a raw sub-slice of the mmap:
`crates/nexus-core/src/storage/graph_engine/engine.rs:432-447` computes
`compressed_data = &self.mmap[offset..end]` where `offset =
index_entry.list_offset as usize` — an on-disk byte offset with **no 8-byte
alignment constraint** — and passes it straight to
`decompress_adjacency_list(.., CompressionType::None, ..)`. Any
`GraphStorageEngine` outgoing/incoming adjacency lookup
(`engine.rs:443`, `:590`) over a segment whose `list_offset` is not a
multiple of 8 executes this UB; `GraphStorageEngine` is wired into the live
compiled-execution path (`crates/nexus-core/src/execution/compiled.rs:8`).
The `lz4`/`zstd` variants decode into a fresh heap `Vec<u8>` first, so they
are *less likely* to be misaligned in practice (allocators commonly return
suitably-aligned memory), but the same construct is still UB whenever the
allocator happens not to guarantee ≥8-byte alignment for that allocation
size/path — it is not a portability guarantee Rust or `Vec` makes.

### Why this is UB, not merely a hazard

`std::slice::from_raw_parts(data, len)`'s documented safety contract
includes "`data` must be non-null and aligned even for zero-length slices"
and "the memory referenced ... must be a single allocated object valid for
reads for `len * size_of::<T>()` bytes ... and it must be properly
aligned." An unaligned `*const AdjacencyEntry` breaks the alignment clause
unconditionally. This is UB by construction of the slice value itself — the
optimizer is entitled to assume the returned slice is aligned and may
miscompile any code downstream of it (not merely "may segfault on strict
architectures"); on x86 it typically "works" because the ISA tolerates
unaligned scalar loads, but that tolerance does not extend to what the
compiler is allowed to assume once a `&[AdjacencyEntry]` value exists.

## What Changes

- Stop transmuting the raw byte slice into a typed slice. Decode each
  8-byte little-endian `rel_id` explicitly via `chunks_exact(8)` +
  `u64::from_le_bytes`, which is alignment-agnostic and also fixes
  endianness portability (the current cast additionally assumes native
  byte order matches the on-disk format).
- Apply the fix uniformly to all three call sites: `decompress_none`
  (`compression.rs:439-441`), `decompress_lz4` (`:604-606`), `decompress_zstd`
  (`:646-648`).
- Alternative considered and rejected as primary fix: mark `AdjacencyEntry`
  `#[repr(C, packed)]` + `bytemuck::Pod` and use `bytemuck::try_cast_slice`
  (which itself validates alignment and errors instead of invoking UB) —
  viable but changes the type's public repr and still pays a validation
  branch per call; the `from_le_bytes` decode avoids both a repr change and
  any unsafe code on this path.

## Impact

- Affected specs: `docs/specs/storage-format.md` (adjacency-list on-disk
  encoding / decompression contract)
- Affected code: `crates/nexus-core/src/storage/graph_engine/compression.rs`
  (`decompress_none:439-441`, `decompress_lz4:604-606`,
  `decompress_zstd:646-648`), `crates/nexus-core/src/storage/graph_engine/format.rs`
  (`AdjacencyEntry:164-169`, informational — no change needed if the
  byte-decode fix is chosen)
- Breaking change: NO — on-disk bytes and public decompression results are
  unchanged; only the internal reconstruction method changes
- User benefit: removes undefined behaviour from every adjacency-list
  traversal (`GraphStorageEngine` outgoing/incoming lookups), eliminating a
  miscompilation/SIGBUS risk that is currently latent rather than reliably
  reproducible
- Related: `phase0_fix-storage-oob-panics` (adjacent storage-layer bounds
  hardening in the same audit pass)
