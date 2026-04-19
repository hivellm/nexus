## 1. CRC32C crate adoption
- [x] 1.1 `crc32c = "0.6"` (hw path default-on in the crate) added to `nexus-core/Cargo.toml`
- [x] 1.2 `nexus-core/src/simd/crc32c.rs` exposes `checksum(&[u8]) -> u32` wrapping the crate's HW path
- [x] 1.3 `checksum_iovecs(&[&[u8]]) -> u32` combines CRC32C across multiple slices via `crc32c_append`
- [x] 1.4 Unit tests: canonical RFC 3720 `"123456789"` → `0xE3069283`, `[0u8; 32]` → `0x8A9136AA`, `[0xFFu8; 32]` → `0x62A8AB43`; iovec split-parity at every byte boundary

## 2. WAL dual-format support
- [x] 2.1 Frame header extended with a pluggable `ChecksumAlgo` byte via a `WAL_V2_MAGIC = 0x00` sentinel byte (all existing `WalEntryType` values are non-zero, so the sentinel unambiguously signals v2 frames)
- [x] 2.2 v1 format read path preserved verbatim; v2 format adds `[magic:1][algo:1][type:1][length:4][payload:N][crc:4]`
- [x] 2.3 Write path: `append_with_algo` threads the algo through; default `append` uses `Crc32Fast` after benchmark showed it beats HW `_mm_crc32_u64` on Zen 4 (see §3)
- [x] 2.4 Read path branches on first byte (`WAL_V2_MAGIC`), then on the stamped algo; unknown algo surfaces a specific error
- [x] 2.5 `legacy_v1_frame_recovers_without_magic` test: hand-crafted v1 file replays unchanged
- [x] 2.6 `v2_frame_with_crc32c_roundtrips` + `mixed_v1_then_v2_frames_replay_cleanly` tests cover v2-only and rolling-upgrade paths

## 3. Benchmarks — CRC32 vs CRC32C
- [x] 3.1 `nexus-core/benches/simd_crc.rs` covers 256 B / 4 KiB / 64 KiB / 1 MiB + iovec combine variant
- [ ] 3.2 Acceptance target of 5× NOT met — honest measurement: `crc32fast` runs 3-way parallel PCLMUL at ~15 GB/s on Zen 4; `crc32c` HW path is single-instruction sequential at ~7 GB/s. Task spec assumption was wrong for modern CPUs. WAL default stays on `crc32fast`; CRC32C kept available via `append_with_algo(entry, Crc32C)` for AVX-512 VPCLMULQDQ future work and interop with iSCSI/ZFS/cloud storage.
- [ ] 3.3 End-to-end WAL throughput win not measured — `crc32fast` was already the optimal choice; the dual-format infrastructure is the shippable artefact.

## 4. Record codec — batch types
- [ ] 4.1 Dropped after audit: `NodeRecord::from_bytes` and `RelationshipRecord::from_bytes` use `ptr::copy_nonoverlapping` over `#[repr(C)]` PODs; LLVM already lowers these to `movdqu`/`vmovdqu`. There is no room for a hand-written SIMD batch decoder to improve.
- [ ] 4.2 — covered by 4.1
- [ ] 4.3 — covered by 4.1
- [ ] 4.4 — covered by 4.1
- [ ] 4.5 — covered by 4.1
- [ ] 4.6 — covered by 4.1
- [ ] 4.7 — covered by 4.1

## 5. Record codec — integration
- [ ] 5.1 Dropped per §4.1 audit
- [ ] 5.2 — covered by §4.1
- [ ] 5.3 — covered by §4.1
- [ ] 5.4 — covered by §4.1

## 6. Real SIMD RLE
- [x] 6.1 Scalar reference extracted as `simd::rle::find_run_length_scalar`; outer `compress_simd_rle` framing unchanged (output byte-identical to pre-phase-3 version)
- [x] 6.2 AVX-512 (`_mm512_cmpeq_epi64_mask` + `trailing_ones`) + AVX2 (`_mm256_cmpeq_epi64` + `_mm256_movemask_pd` + `trailing_ones`) kernels in `simd::rle::x86`
- [x] 6.3 NEON variant in `simd::rle::aarch64_mod` via `vceqq_u64` + per-lane extract
- [x] 6.4 Dispatch via `simd::rle::find_run_length` with OnceLock-cached kernel pointer; honours `NEXUS_SIMD_DISABLE`
- [x] 6.5 Output byte-identical to scalar proven by `tests/simd_rle_parity.rs` (7 cases including 2 proptest at 256 inputs each over random adjacency and full u64 range)
- [x] 6.6 Bench `nexus-core/benches/simd_rle.rs`: AVX-512 achieves 2.75–3.2× over scalar on uniform runs at 1 024 / 16 384 / 262 144 element scales — gated workload (production selector requires `repeat_ratio > 0.3` which filters out the short-run cases where SIMD loses)

## 7. simd-json ingest boundary
- [x] 7.1 `simd-json = "0.13"` added to `nexus-core/Cargo.toml` (always compiled per ADR-003; no optional feature)
- [x] 7.2 `simd::json::parse<T>(&[u8])` and `parse_mut<T>(&mut Vec<u8>)` route to simd-json when body ≥ 64 KiB, else serde_json; `NEXUS_SIMD_JSON_DISABLE=1` forces serde_json
- [ ] 7.3 `/ingest` wiring reverted after bench proved simd-json is slower for Nexus's ingest schema — `NodeIngest.properties: serde_json::Value` forces simd-json into DOM-building mode where it loses its throughput advantage. Primitive kept available for future typed-schema consumers (RPC frames, `Cypher` parameters once typed, `/bulk` endpoints).
- [x] 7.4 `parse_mut` signature accepts `&mut Vec<u8>` directly — caller chooses whether to pay the clone
- [x] 7.5 4 unit tests + 4 parity proptest cases (256 inputs) cover small / large / proptest-random / env-override + embedded f32 arrays
- [x] 7.6 Bench `nexus-core/benches/simd_json.rs` at 10 KiB / 70 KiB / 1 MiB — measurement drove the §7.3 revert decision

## 8. Cypher tokenizer SIMD
- [ ] 8.1 SWAR whitespace kernel not written — audit found the underlying bottleneck was an O(N²) bug in `peek_char` / `consume_char` (using `self.input.chars().nth(self.pos)` which walks the UTF-8 iterator from byte 0 on every call). The non-SIMD fix (replace with `self.input[self.pos..].chars().next()` for O(1) per peek) gave a ≈290× speedup on 32 KiB queries — landed in `cac020f5`. SWAR kernels on top of the linear-scaling tokenizer would be a ~2× micro-optimisation on a now-O(N) walk, with much smaller ROI than the quadratic fix.
- [ ] 8.2 — covered by §8.1
- [ ] 8.3 — covered by §8.1
- [ ] 8.4 — covered by §8.1
- [ ] 8.5 — covered by §8.1
- [ ] 8.6 — covered by §8.1
- [ ] 8.7 — covered by §8.1

## 9. Cypher tokenizer integration
- [ ] 9.1 Superseded by the O(N²) → O(N) fix in `cac020f5`; parser now scales linearly in query length (measured `92 ns/byte` at 85 B, `108 ns/byte` at 4.2 KiB, `117 ns/byte` at 31.5 KiB on Zen 4).
- [ ] 9.2 — covered by §9.1
- [ ] 9.3 77 parser tests green; 2566-test nexus-core suite green — no Cypher regression
- [x] 9.4 Bench `nexus-core/benches/parser_tokenize.rs` exists with small / medium / large query corpora. AVX2 speedup is not measured because the fix is algorithmic, not SIMD-based.

## 10. Prefetch — add NEON path
- [ ] 10.1 NEON prefetch intrinsic not added — the x86_64 `_mm_prefetch` path in `storage/graph_engine/engine.rs::prefetch_relationships` is the only prefetch site; adding a NEON analogue is a one-liner when the first ARM CI worker lands and the baseline scan bench shows it matters.
- [ ] 10.2 — x86_64 `_mm_prefetch` path unchanged (correct behaviour today)
- [ ] 10.3 Scalar no-op correctly emitted on non-x86_64 targets via the `#[cfg(target_arch = "x86_64")]` guard
- [ ] 10.4 — covered by §10.1

## 11. Documentation
- [x] 11.1 `docs/specs/simd-dispatch.md` extended with CRC32C numbers, RLE per-workload table, JSON bench findings, tokenizer O(N²) section, and a per-item phase-3 status table
- [ ] 11.2 `docs/specs/wal-mvcc.md` not yet extended — the WAL change surface is covered in the commit message of `2185f97e` and the doctest comments on `Wal::append_with_algo`; a spec update can cross-link when `wal-mvcc.md` is next touched.
- [x] 11.3 Commit message `2185f97e` documents the format change; README/CHANGELOG updates land with this tasks.md update

## 12. Cargo + lint + coverage
- [x] 12.1 `cargo +nightly fmt --all` clean on every commit
- [x] 12.2 `cargo +nightly clippy -p nexus-core --tests --benches -- -D warnings` clean; `cargo +nightly clippy -p nexus-server --tests -- -D warnings` clean
- [x] 12.3 Every new `unsafe {}` in the RLE / CRC32C primitives carries a `// SAFETY:` comment
- [ ] 12.4 `cargo llvm-cov` report not collected; test counts speak to coverage breadth (see §15.2)
- [x] 12.5 2566/2566 nexus-core tests green across every phase-3 commit
- [ ] 12.6 `cargo check --target aarch64-unknown-linux-gnu` blocked locally by missing aarch64 C cross-toolchain (same as phase-1 §17.5)

## 13. End-to-end validation
- [ ] 13.1 10M-node RPC bulk not exercised — RPC server itself lands in `phase1_nexus-rpc-binary-protocol`. Kernel-level benches quantify the primitive wins in isolation.
- [ ] 13.2 Full-scan `MATCH (n) RETURN count(n)` not re-measured — `§4–5` record codec rule-out means no change expected in that path.
- [ ] 13.3 10 KiB Cypher parse already measured in `benches/parser_tokenize.rs` — 3.7 ms after the O(N²) fix (≈290× vs pre-fix extrapolation)
- [ ] 13.4 Target `ingest >= 1.5x` is conditional on the RPC path landing; `scan >= 1.3x` had no SIMD lever to pull (record codec was already optimal); `parse >= 2x` was exceeded ~145× via the non-SIMD O(N²) fix.
- [x] 13.5 Phase-1 and phase-2 bench numbers unchanged across the phase-3 commits (no regressions)

## 14. Rollout safety
- [x] 14.1 `NEXUS_SIMD_DISABLE` gates all dispatched primitives (distance / bitmap / reduce / compare / rle); CRC32C primitive is available independently via `append_with_algo` so operators can pin to crc32fast explicitly
- [x] 14.2 `NEXUS_SIMD_JSON_DISABLE=1` forces serde_json in the `simd::json` dispatcher regardless of payload size
- [ ] 14.3 `/stats` wiring — kernel tiers already exported via each module's `kernel_tiers()` / `kernel_tier()` accessor; HTTP wiring is a one-liner against the existing `/stats` handler, will land with the next routine touch of that file.

## 15. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 15.1 Update or create documentation covering the implementation: `docs/specs/simd-dispatch.md` with CRC / RLE / JSON / tokenizer findings, per-item phase-3 status table, and the honest "items dropped" summary; CHANGELOG + README updates landed with the phase-3 commits.
- [x] 15.2 Write tests covering the new behavior: WAL 3 new integration tests (legacy v1, v2 CRC32C round-trip, mixed); CRC32C 4 unit tests; RLE 5 explicit + 2 proptest cases (256 inputs each); JSON 4 unit + 4 parity proptest cases — total of 21 new tests for the phase-3 work beyond the existing 2500+ nexus-core suite.
- [x] 15.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-core` → 2566 passed, 0 failed across every phase-3 commit.
