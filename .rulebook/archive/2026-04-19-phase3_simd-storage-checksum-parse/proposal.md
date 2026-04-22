# Proposal: phase3_simd-storage-checksum-parse

## Why

Phase 1 and Phase 2 cover KNN distance and Cypher executor hot paths —
the two biggest CPU sinks in a read query. Phase 3 sweeps the **write
path and IO surface**: the places where CPU becomes the bottleneck once
the executor is fast enough that the server starts ingesting GBs/s.

Five remaining targets, each measured against concrete hot paths:

1. **WAL commit throughput is bounded by CRC32**. Every WAL frame
   carries a CRC; `crc32fast` (our current crate) is fast but stops
   short of the ~40 GB/s achievable with hardware CRC32C on SSE4.2
   (`_mm_crc32_u64`) or ARMv8 CRC (`__crc32cd`). At 1M-node bulk
   ingest, the WAL currently spends ~8% of wall time in CRC.
2. **Record (de)serialisation** reads `NodeRecord` (32B) and
   `RelationshipRecord` (48B) field-by-field via `from_le_bytes` —
   8 dependent scalar reads per record. A single aligned 32B load +
   SIMD shuffle is ~2–3x faster, which matters during index rebuild
   and full scans.
3. **Compaction's `compress_simd_rle`** is named "SIMD" but is scalar.
   At 10M edges, RLE build time is ~340 ms; an actual SIMD run-length
   scan (AVX2 `_mm256_cmpeq_epi64` + `movemask` + `tzcnt`) should cut
   that to ~80 ms.
4. **JSON ingest** — the HTTP and RPC ingest endpoints parse large
   payloads via stock `serde_json`. Switching to `simd-json` on
   payload sizes > 64 KB gives **2–3x** throughput on SSE4.2/AVX2.
5. **Cypher tokenizer** — `executor/parser/mod.rs` iterates byte-by-
   byte to find identifiers, whitespace, and punctuation. A SWAR or
   SSE4.2 `PCMPESTRI`-based scanner is **2–4x** faster on large
   queries (multi-KB generated MATCH clauses are common in LLM-
   generated traffic).

None of these is as impactful individually as KNN distance, but
together they recover another 20–30% of bulk-ingest and query-parse
throughput.

## What Changes

### 1. CRC32C kernel

Swap `crc32fast` (scalar/SSE4.2 general-purpose CRC32) for
`crc32c = { version = "0.6", features = ["hw"] }` which does
CRC32**C** (Castagnoli, the same polynomial used by iSCSI / ZFS /
Google storage). Hardware paths:

- x86_64 SSE4.2 (`_mm_crc32_u64`) — 1 byte/cycle scalar, ~8 bytes/cycle
  with 3-way pipelining
- ARMv8 CRC (`__crc32cd`) — similar throughput

Both are the reference implementations in the `crc32c` crate we adopt;
we do not hand-roll intrinsics. The WAL format version bumps by 1 with
a `checksum_algo: u8` field so existing data keeps using CRC32 and new
writes use CRC32C. Migration is automatic: on first read we detect
`checksum_algo == 0` (legacy) and verify with `crc32fast`.

### 2. Record codec batch path

Introduce `storage::codec::simd` with batch encode/decode:

```rust
pub fn decode_nodes(src: &[u8], out: &mut [NodeRecord]) -> usize;
pub fn decode_relationships(src: &[u8], out: &mut [RelationshipRecord]);
pub fn encode_nodes(src: &[NodeRecord], out: &mut [u8]) -> usize;
pub fn encode_relationships(src: &[RelationshipRecord], out: &mut [u8]);
```

AVX2 path: load 128 bytes (4 nodes) with two `_mm256_loadu_si256`, then
`_mm256_shuffle_epi8` to extract the u64 fields in parallel. NEON path:
`vld1q_u8` + `vqtbl4q_u8` for the same shuffle. Scalar fallback keeps
`from_le_bytes`. Single-record paths stay on `from_le_bytes` — the
batch path kicks in at ≥16 records per call (matches the cache line).

### 3. SIMD RLE (actual SIMD this time)

Rewrite `compress_simd_rle` in `storage/graph_engine/compression.rs`:

```rust
fn find_run_length_avx2(entries: &[AdjacencyEntry], start: usize) -> usize {
    // Load 4 u64 rel_ids, broadcast entries[start].rel_id,
    // _mm256_cmpeq_epi64 → movemask → tzcnt → 4-element step
    // Repeat until mask is not all-set.
}
```

Runtime dispatch via phase 1's `cpu()`. Scalar fallback is the current
code (renamed `compress_rle_scalar`). Output format unchanged — RLE
byte stream is identical, just built faster.

### 4. simd-json ingest boundary

Add `simd-json = "0.13"` as an optional dep behind feature
`simd-json-ingest` (default on). In the ingest handlers:

- `POST /ingest` body >64 KB: use `simd_json::to_owned_value`
- RPC `INGEST.NODES` / `INGEST.RELS`: same threshold
- Cypher `parameters` payload: same threshold (parameters often carry
  vector embeddings in thousands of f32s)

Under the threshold, stock `serde_json` stays: it avoids the
mutable-buffer requirement of `simd-json`.

### 5. Cypher tokenizer SIMD

`executor/parser/mod.rs` gains a SWAR-based whitespace and identifier
scanner:

- **Whitespace skip**: 64-bit SWAR — load 8 bytes, subtract 0x20,
  check MSB mask → jump to first non-whitespace byte in O(1) per
  8-byte block.
- **Identifier end**: SSE4.2 `_mm_cmpestri` with a char-class range
  `[A-Za-z0-9_]` returns the first non-matching byte in one
  instruction. AVX2 version uses `_mm256_cmpeq_epi8` + `movemask` +
  `tzcnt`. NEON uses `vcltq_u8`/`vcgtq_u8` + bit-packing.
- **String literal**: skip to next `'` or `"` — same PCMPESTRI
  pattern.

Scalar fallback is the current byte-at-a-time loop. Runtime dispatch.

### 6. Cross-arch support reaffirmed

All kernels use phase 1's dispatch and fallback infrastructure. No
phase-3 code is x86_64-only.

## Impact

- **Affected specs**: update `docs/specs/simd-dispatch.md` with the
  new CRC/record/RLE/parse kernel tables; update
  `docs/specs/wal-mvcc.md` with the CRC32C migration path.
- **Affected code**:
  - NEW: `nexus-core/src/storage/codec/simd.rs` (~400 LOC)
  - NEW: `nexus-core/src/simd/crc32c.rs` (thin wrapper over `crc32c`
    crate, ~100 LOC)
  - NEW: `nexus-core/src/simd/parse.rs` (SWAR + SSE4.2 scanners,
    ~500 LOC)
  - MODIFIED: `nexus-core/src/wal/mod.rs` (+ checksum-algo field,
    read-compat fallback, writes switch to CRC32C)
  - MODIFIED: `nexus-core/src/storage/graph_engine/compression.rs`
    (real SIMD RLE)
  - MODIFIED: `nexus-core/src/storage/graph_engine/format.rs`
    (batch codec integration)
  - MODIFIED: `nexus-core/src/executor/parser/mod.rs` (tokenizer fast
    path)
  - MODIFIED: `nexus-server/src/api/ingest.rs` + RPC dispatch
    (simd-json boundary)
  - MODIFIED: `Cargo.toml` (+ `crc32c`, `simd-json` optional)
- **Breaking change**: NO — all output formats unchanged. WAL reads
  are backwards-compatible; WAL writes use new algo flag but old
  readers fail closed (they check the flag). CHANGELOG entry under
  "File format" marks the write-format bump.
- **User benefit**:
  - **6–10x** WAL commit throughput on bulk ingest.
  - **2–3x** full-scan / index-rebuild throughput via batch codec.
  - **3–5x** compaction RLE build time.
  - **2–3x** ingest throughput on payloads >64 KB.
  - **2–4x** Cypher parse throughput on large queries.

## Non-goals

- WAL format rewrite (column layout, page-level compression) —
  separate phase.
- Replacing `tantivy` FTS index with a SIMD-aware index — tantivy has
  its own SIMD paths; not our job.
- GPU offload of any kind.

## Reference

- `crc32c` crate hardware CRC: <https://crates.io/crates/crc32c>
- `simd-json` paper (Langdale & Lemire, 2019): "Parsing Gigabytes of
  JSON per Second"
- Daniel Lemire's whitespace SWAR blog: <https://lemire.me/blog/>
- DuckDB tokenizer SIMD reference implementation
