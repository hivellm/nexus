## 1. CRC32C crate adoption
- [ ] 1.1 Add `crc32c = { version = "0.6", features = ["hw"] }` to `nexus-core/Cargo.toml`
- [ ] 1.2 Create `nexus-core/src/simd/crc32c.rs` exposing `checksum(&[u8]) -> u32` with the `crc32c` crate under the hood
- [ ] 1.3 Add a `crc32c_batch(iovecs: &[&[u8]])` helper that combines CRC32C across multiple slices without buffer concat
- [ ] 1.4 Unit test: CRC32C matches a known vector (`0xE3069283` for the ASCII string `"123456789"`)

## 2. WAL dual-format support
- [ ] 2.1 Extend the WAL frame header with `checksum_algo: u8` (0 = CRC32 legacy, 1 = CRC32C)
- [ ] 2.2 Bump the WAL format version constant; keep the old constant in the read path for backward compat
- [ ] 2.3 Write path: always emit `checksum_algo = 1` and use `simd::crc32c::checksum`
- [ ] 2.4 Read path: branch on `checksum_algo`, verify with the matching function, surface a clear error when the algo is unknown
- [ ] 2.5 Integration test: old WAL file written with `crc32fast` still replays correctly after the upgrade
- [ ] 2.6 Integration test: new WAL file written with CRC32C replays correctly after a restart

## 3. Benchmarks — CRC32 vs CRC32C
- [ ] 3.1 `nexus-core/benches/simd_crc.rs`: 4KB, 64KB, 1MB buffer sizes
- [ ] 3.2 Acceptance: CRC32C (hw) >= 5x faster than crc32fast scalar on an SSE4.2 machine
- [ ] 3.3 End-to-end: 1M-node bulk ingest throughput >= 1.3x via WAL path (non-CRC costs dilute the raw gain)

## 4. Record codec — batch types
- [ ] 4.1 Create `nexus-core/src/storage/codec/mod.rs` with `decode_nodes`, `decode_relationships`, `encode_nodes`, `encode_relationships` trait
- [ ] 4.2 Scalar impl: wraps the existing `from_le_bytes` loop
- [ ] 4.3 AVX2 impl using `_mm256_loadu_si256` + `_mm256_shuffle_epi8` for 4 NodeRecords per iter
- [ ] 4.4 NEON impl using `vld1q_u8` + `vqtbl4q_u8`
- [ ] 4.5 AVX-512 impl using `_mm512_loadu_si512` + `_mm512_permutexvar_epi8` for 8 NodeRecords per iter
- [ ] 4.6 Dispatch wiring via `simd::cpu()` feature flags
- [ ] 4.7 proptest: batch codec round-trips match scalar for every record variant

## 5. Record codec — integration
- [ ] 5.1 Replace the per-record `from_bytes` loop in `storage/graph_engine/engine.rs::prefetch_relationships` and callers with the batch codec when >= 16 records
- [ ] 5.2 Scan-heavy paths (full-scan executor, index rebuild) call batch decode
- [ ] 5.3 Single-record reads keep the scalar helper (tail)
- [ ] 5.4 Unit tests: scan 100K nodes returns identical records via scalar and batch

## 6. Real SIMD RLE
- [ ] 6.1 Rename the existing `compress_simd_rle` to `compress_rle_scalar` (it was misnamed)
- [ ] 6.2 New `compress_rle_simd` in `storage/graph_engine/compression.rs` using AVX2 `_mm256_cmpeq_epi64` + `_mm256_movemask_epi8` + `tzcnt` for run detection
- [ ] 6.3 NEON variant using `vceqq_u64` + `vshrn_n_u64` to produce a packed mask + count leading ones
- [ ] 6.4 Dispatch: `compress_rle(entries)` picks scalar / AVX2 / NEON via `cpu()`
- [ ] 6.5 Output byte stream bit-identical to scalar — assert via proptest on 1M synthetic entries
- [ ] 6.6 Bench: `storage_compression.rs` at 1M entries; acceptance: AVX2 >= 3x scalar

## 7. simd-json ingest boundary
- [ ] 7.1 Add optional dep: `simd-json = { version = "0.13", optional = true }` and feature `simd-json-ingest = ["simd-json"]` default-on
- [ ] 7.2 Wrapper `nexus_server::ingest::json::parse(body: &mut [u8]) -> Result<Value>` picks simd-json when body >= 64 KB, else serde_json
- [ ] 7.3 Apply at three call sites: `POST /ingest`, RPC `INGEST.NODES` / `INGEST.RELS`, Cypher `parameters` payload
- [ ] 7.4 Ensure the input buffer is mutable (simd-json requires it); existing axum extractors copy into a Vec<u8>
- [ ] 7.5 Unit tests: parse output equals serde_json for each of: small body, 100 KB body, 10 MB body with embedded f32 arrays
- [ ] 7.6 Bench: `ingest_parse.rs` at 64 KB / 1 MB / 10 MB; acceptance: simd-json >= 2x serde_json at >= 1 MB

## 8. Cypher tokenizer SIMD
- [ ] 8.1 Create `nexus-core/src/simd/parse.rs` with `skip_whitespace_swar(&[u8], pos) -> usize` (8-byte SWAR)
- [ ] 8.2 Add `scan_identifier_end_sse42` using `_mm_cmpestri` with a char-class range
- [ ] 8.3 AVX2 variant using `_mm256_cmpeq_epi8` + `_mm256_movemask_epi8` + `tzcnt` on multiple ranges
- [ ] 8.4 NEON variant using `vcltq_u8`/`vcgtq_u8` + `vshrn_n_u16` bit-pack
- [ ] 8.5 `scan_until_quote(&[u8], pos, quote: u8)` for string literal termination
- [ ] 8.6 Dispatch layer picks SSE4.2 / AVX2 / NEON / scalar via `cpu()`
- [ ] 8.7 proptest: every scanner output matches the byte-at-a-time scalar on random ASCII buffers and multi-byte UTF-8

## 9. Cypher tokenizer integration
- [ ] 9.1 Rewire `executor/parser/mod.rs` tokenizer to call the SIMD scanners when query length >= 256 bytes
- [ ] 9.2 Fallback: scalar path for queries < 256 bytes (one-shot cost dominated by overhead)
- [ ] 9.3 Unit tests: parse 10 large queries (multi-KB MATCH + WHERE), confirm tokens identical to scalar path
- [ ] 9.4 Bench: `parser_tokenize.rs` at 1 KB / 16 KB query; acceptance: AVX2 >= 2x scalar

## 10. Prefetch — add NEON path
- [ ] 10.1 Extend `storage/graph_engine/engine.rs::prefetch_relationships` with an `#[cfg(target_arch = "aarch64")]` branch using `core::arch::aarch64::__pld` (or inline asm `prfm pldl1keep`)
- [ ] 10.2 Keep x86_64 `_mm_prefetch` path unchanged
- [ ] 10.3 Scalar no-op on other arches
- [ ] 10.4 Unit test via `cfg_attr(not(miri), ignore)` — prefetch is unobservable in miri

## 11. Documentation
- [ ] 11.1 Extend `docs/specs/simd-dispatch.md` with CRC32C, batch codec, RLE, JSON, parser kernel tables
- [ ] 11.2 Update `docs/specs/wal-mvcc.md` — CRC32C migration path (write CRC32C, read both)
- [ ] 11.3 CHANGELOG entry under "File format" announcing the WAL checksum-algo bump

## 12. Cargo + lint + coverage
- [ ] 12.1 `cargo +nightly fmt --all` clean
- [ ] 12.2 `cargo clippy --workspace --all-features -- -D warnings` clean
- [ ] 12.3 Every new `unsafe {}` block carries a `// SAFETY:` comment
- [ ] 12.4 `cargo llvm-cov --package nexus-core` >= 95% on new files
- [ ] 12.5 300/300 Neo4j compat suite still green
- [ ] 12.6 `cargo check --target aarch64-unknown-linux-gnu` clean

## 13. End-to-end validation
- [ ] 13.1 Ingest 10M nodes + 50M relationships via RPC bulk; record wall time and WAL bytes/s
- [ ] 13.2 Scan-all via `MATCH (n) RETURN count(n)` — record wall time
- [ ] 13.3 Complex Cypher pattern query (10KB query body) — record parse + execute time
- [ ] 13.4 Compare vs pre-phase-3 numbers; acceptance: ingest >= 1.5x, scan >= 1.3x, parse >= 2x
- [ ] 13.5 No regressions in Phase 1 KNN benchmarks or Phase 2 filter/aggregate benchmarks

## 14. Rollout safety
- [ ] 14.1 `NEXUS_SIMD_DISABLE` also disables CRC32C (falls back to crc32fast) and codec/RLE/parse SIMD paths
- [ ] 14.2 `NEXUS_SIMD_JSON_DISABLE` env forces serde_json regardless of payload size — for customers hitting simd-json compat edge cases
- [ ] 14.3 `/stats` exposes: `simd.crc_kernel`, `simd.record_codec`, `simd.rle_kernel`, `ingest.parser = simd-json|serde_json`

## 15. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 15.1 Update or create documentation covering the implementation (`docs/specs/simd-dispatch.md` + `docs/specs/wal-mvcc.md` + CHANGELOG)
- [ ] 15.2 Write tests covering the new behavior (WAL migration, codec roundtrip, RLE parity, JSON parser parity, tokenizer parity; >= 50 tests total)
- [ ] 15.3 Run tests and confirm they pass (`cargo test --workspace --all-features --verbose`)
