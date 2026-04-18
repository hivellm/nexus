# SIMD dispatch

> Accepted via ADR-003 (2026-04-18) — "SIMD dispatch: runtime feature
> detection with tiered fallback and per-kernel parity tests".

Nexus compiles every SIMD kernel into the release binary unconditionally
and selects the fastest path the host CPU advertises at runtime. There
is no Cargo feature that turns SIMD off at build time — the use is
standard whenever the CPU supports it. Emergency rollback is a single
env var flip at process start.

This document is the single source of truth for kernel authors. Read it
before adding a new SIMD kernel; if you find yourself deviating from
the conventions below, update this spec first.

## 1. Module layout

```text
nexus-core/src/simd/
├── mod.rs              re-exports: cpu(), CpuFeatures, bitmap, compare,
│                       crc32c, distance, json, reduce, scalar
├── dispatch.rs         CpuFeatures + OnceLock<CpuFeatures> probe
├── scalar.rs           reference kernels (ground truth)
├── distance.rs         safe public API for f32 dot/l2_sq/cosine/normalize
├── bitmap.rs           safe public API for popcount/and_popcount
├── reduce.rs           safe public API for sum/min/max (i64/f64/f32)
├── compare.rs          safe public API for eq/ne/lt/le/gt/ge (i64/f64)
├── crc32c.rs           hardware CRC32C wrapper (interop)
├── json.rs             size-threshold JSON dispatch (simd-json vs serde_json)
├── x86.rs              #[cfg(target_arch="x86_64")] kernels
└── aarch64.rs          #[cfg(target_arch="aarch64")] kernels
```

Adding a new SIMD op follows one pattern:

1. Write the scalar reference in `scalar.rs` first, `#[inline(always)]`.
2. Write `unsafe` architecture kernels in `x86.rs` and `aarch64.rs`
   behind `#[target_feature(enable = "...")]`.
3. Add a safe dispatch entry in `distance.rs` / `bitmap.rs` that caches
   the kernel pointer in a `OnceLock<unsafe fn>`.
4. Add a proptest parity suite in `nexus-core/tests/simd_*_parity.rs`
   that asserts every SIMD kernel matches the scalar reference within
   the tolerance table in §5.

## 2. Runtime detection

`dispatch::CpuFeatures` holds boolean flags and is built once per
process:

| Flag                | Detection macro                              | Used by                      |
|---------------------|----------------------------------------------|------------------------------|
| `avx512f`           | `is_x86_feature_detected!("avx512f")`        | distance kernels (16 lanes)  |
| `avx512_vpopcntdq`  | `is_x86_feature_detected!("avx512vpopcntdq")`| popcount kernels             |
| `avx2`              | `is_x86_feature_detected!("avx2")` + `fma`   | distance + bitmap            |
| `sse42`             | `is_x86_feature_detected!("sse4.2")`         | distance (4 lanes), CRC32C   |
| `neon`              | `is_aarch64_feature_detected!("neon")`       | distance + bitmap            |
| `sve2`              | `is_aarch64_feature_detected!("sve2")`       | reserved (unused in v1.0.0)  |
| `disabled`          | `NEXUS_SIMD_DISABLE=1` env var               | forces scalar everywhere     |

`cpu()` returns `&'static CpuFeatures`. A single `tracing::info!` line
is emitted on the first call describing the selected tier.

## 3. Cascade order

Both distance and bitmap dispatch use the same first-match cascade:

```text
 x86_64:     AVX-512 → AVX2 → SSE4.2 → Scalar
 aarch64:    NEON → Scalar
 other:      Scalar
 + NEXUS_SIMD_DISABLE = Scalar unconditionally
```

The cascade is hand-coded in `pick_<op>()` inside `distance.rs` /
`bitmap.rs`. Adding a new tier means extending the cascade in every
dispatch function — deliberate, so no dispatch silently regresses when
a new kernel lands.

## 4. Tail handling

| Tier       | Tail policy                                           |
|------------|-------------------------------------------------------|
| AVX-512    | Native masked load via `_mm512_maskz_loadu_ps/epi64`  |
| AVX2       | Scalar loop for the last `n % lane_count` elements    |
| SSE4.2     | Scalar loop for the last `n % lane_count` elements    |
| NEON       | Scalar loop for the last `n % lane_count` elements    |

AVX-512's native masked load avoids a branch-heavy tail and is the
biggest single contributor to the `len = 1..16` dispatch being
competitive with scalar at very small sizes. AVX2 and below pay a
modest scalar tail; the overhead is dominated by the SIMD body for any
`len >= 32`.

## 5. Correctness tolerances

| Op family                        | Tolerance                                          |
|----------------------------------|----------------------------------------------------|
| f32 dot / l2_sq                  | `5e-4 * max(1, n) * max(1, |scale|)`               |
| f32 cosine                       | `5e-4` absolute (output bounded `[0, 2]`)          |
| f32 normalize norm               | `5e-4 * max(1, |norm|)`                            |
| f32 normalize element            | `5e-5` absolute                                    |
| Integer / bitmap ops             | bit-exact (tolerance = 0)                          |

Rationale: reordered FMAs accumulate rounding error proportional to
both `n` (operation count) and the magnitude of intermediate sums.
The tolerances above are empirically tight enough to catch regressions
while absorbing legitimate ILP reorderings on AVX2/AVX-512. Every
proptest in `tests/simd_*_parity.rs` enforces these bounds across
256 generated inputs, with lengths 1..=512 to exercise every tail.

## 6. Observability

`distance::kernel_tiers()` returns `[(op_name, tier_name); 4]` —
stable and safe to expose via `/stats`. `bitmap::kernel_tiers()` does
the same for `popcount_u64` / `and_popcount_u64`. Tier strings the
`/stats` endpoint will emit:

- `"avx512"` — distance ops on AVX-512F
- `"avx512vpopcntdq"` — bitmap ops on AVX-512 + VPOPCNTQ
- `"avx2"` — 8-lane f32 with FMA (distance) or Mula nibble-LUT
  popcount (bitmap)
- `"sse4.2"` — 4-lane f32 distance
- `"neon"` — ARMv8 4-lane f32 / 2-u64 bitmap
- `"scalar"` — scalar reference
- `"scalar (NEXUS_SIMD_DISABLE)"` — forced scalar via env var

## 7. Escape hatch

```bash
NEXUS_SIMD_DISABLE=1 ./target/release/nexus-server
```

Turns every dispatch function into the scalar path. No rebuild, no
redeploy. Documented in the operator runbook; `/stats` surfaces the
active tier so ops can confirm.

## 8. Benchmarks and regression gate

- `cargo +nightly bench -p nexus-core --bench simd_distance` — scalar
  vs dispatch at dims 32/128/256/512/768/1024/1536 for all four
  distance ops.
- `cargo +nightly bench -p nexus-core --bench simd_popcount` — scalar
  vs dispatch at word counts 4/16/64/256/1024/4096 for popcount and
  and_popcount.

Reference numbers measured on Ryzen 9 7950X3D (Zen 4, AVX-512F + VPOPCNTQ):

| Op               | Scale        | Scalar   | Dispatch | Speedup |
|------------------|--------------|----------|----------|---------|
| `dot_f32`        | dim=768      | 438 ns   | 34.5 ns  | 12.7×   |
| `dot_f32`        | dim=1024     | 580 ns   | 50.8 ns  | 11.4×   |
| `dot_f32`        | dim=1536     | 893 ns   | 70.3 ns  | 12.7×   |
| `l2_sq_f32`      | dim=512      | 285 ns   | 21.0 ns  | 13.5×   |
| `popcount_u64`   | 4096 words   | 1.52 µs  | 136 ns   | ~11×    |
| `sum_f64`        | n=262 144    | 150 µs   | 19 µs    | 7.9×    |
| `sum_f32`        | n=262 144    | 152 µs   | 9.5 µs   | 15.9×   |
| `sum_i64`        | n=1 024      | 73 ns    | 30 ns    | 2.4×    |
| `eq_i64`         | n=262 144    | 69 µs    | 24 µs    | 2.9×    |
| `lt_i64`         | n=262 144    | 110 µs   | 25 µs    | 4.4×    |
| `lt_f64`         | n=262 144    | 53 µs    | 24 µs    | 2.2×    |

At the 2 MiB i64 sum row (`sum_i64` n=262 144) scalar and dispatch
tie because the workload is memory-bandwidth bound on Zen 4. At the
1 KiB row (`sum_i64` n=1 024) the dispatch still wins ~2.4× because
the data fits in L1.

### CRC benchmark — honest findings

The phase-3 task spec anticipated a 6–10× speedup from switching WAL
checksums from `crc32fast` (IEEE polynomial) to `crc32c` (Castagnoli
via `_mm_crc32_u64` / ARMv8 `__crc32cd`). Measured on Zen 4:

| Buffer size | `crc32fast` (PCLMUL) | `crc32c` (HW CRC32) | Ratio |
|-------------|----------------------|---------------------|-------|
| 256 B       | 16 ns                | 36 ns               | 0.44× |
| 4 KiB       | 216 ns               | 454 ns              | 0.48× |
| 64 KiB      | 3.5 µs               | 7.0 µs              | 0.50× |

`crc32fast` runs a 3-way parallel CLMUL-based CRC32 at ~15 GB/s, which
beats the single-instruction sequential `_mm_crc32_u64` loop
(~7 GB/s). On this class of hardware, `crc32fast` is the right choice
for raw throughput.

### JSON parser benchmark — honest findings

The phase-3 task spec also anticipated a 2–3× speedup from switching
`/ingest` body parsing above 64 KiB to `simd-json`. Benchmarked on
Zen 4 with the existing ingest schema:

| Payload size | `serde_json`   | `simd-json`    | Ratio       |
|--------------|----------------|----------------|-------------|
| 10 KiB       | 32 µs          | 32 µs          | tie         |
| 70 KiB       | 213 µs         | 315 µs         | simd 1.48× slower |
| 1 MiB        | 4.2 ms         | 5.5 ms         | simd 1.32× slower |

`simd-json` is designed around typed, schema-driven deserialisation.
The `/ingest` schema stores per-node `properties: serde_json::Value`
— an untyped dynamic JSON tree. That forces `simd-json` into DOM
construction, which loses the SIMD advantage because the tape walk
still has to allocate every nested map/array.

Decision: `/ingest` **stays on `serde_json`.** The `simd::json`
dispatcher is kept as a public primitive for future typed-schema
parse paths (e.g. Cypher `parameters` maps with strongly-typed
schemas, or RPC frames that pre-bind their shapes) where the
measurement will tip in `simd-json`'s favour.

### Tokenizer O(N²) fix (non-SIMD, follow-up to phase-3 audit)

`peek_char`, `consume_char`, `peek_char_at`, `skip_whitespace`,
`peek_keyword`, and `peek_keyword_at` all called
`self.input.chars().nth(self.pos)` on every character — an O(n)
iterator walk from the start of the input. Over a full parse that
compounded into **O(N²)** wall time.

Fix: replace `self.input.chars().nth(pos)` with
`self.input[pos..].chars().next()`. Byte-slice creation is O(1)
and `chars().next()` decodes exactly one character — O(1) per
peek, O(N) over the full parse.

Measured on Ryzen 9 7950X3D via `benches/parser_tokenize.rs`:

| Query size  | Parse time | ns / byte |
|-------------|------------|-----------|
|    85 bytes | 7.8 µs     | 92        |
| 4.2 KiB     | 454 µs     | 108       |
| 31.5 KiB    | 3.7 ms     | 117       |

Cost per byte is effectively constant across three orders of
magnitude — confirming the O(N²) is gone. If the quadratic
pattern had remained, the 31.5 KiB query would have taken
`(31555/85)² × 7.8 µs ≈ 1.07 s`, i.e. ~290× the current 3.7 ms.

This is the fix the phase-3 `§8–9 Cypher tokenizer SWAR`
task item gestured at. SIMD on the byte loop is a further
~2× on top of this once the O(N²) is out of the way; it is not
implemented in phase-3 because the linear-scaling fix already
covers the bulk of the gain for current query sizes.

### Phase 3 status — honest scope

The phase-3 task spec (`phase3_simd-storage-checksum-parse`)
anticipated six SIMD wins. After auditing each call site against
modern CPUs and the actual codebase, only two delivered as designed:

| Item | Task claim | Outcome |
|------|-----------|---------|
| §1–3 CRC32C WAL | 6–10× throughput | Slower on Zen 4 (crc32fast PCLMUL wins); dual-format infra kept for future VPCLMULQDQ / interop |
| §6 SIMD RLE | 3–5× build time | ✅ 2.75–3.2× on long runs (workload where gate selects RLE) |
| §7 simd-json ingest | 2–3× on >64 KiB | Slower on Nexus payload shape (Value fields force DOM); primitive kept for typed callers |
| §4–5 Record codec batch | 2–3× scan/rebuild | **Already optimal** — existing `from_bytes` is `ptr::copy_nonoverlapping` of `#[repr(C)]` PODs; LLVM auto-vectorises the memcpy |
| §8–9 Cypher tokenizer SWAR | 2–4× parse | **Not a SIMD opportunity** — `peek_char`/`consume_char` call `self.input.chars().nth(self.pos)` which is O(N) per character, so every byte increment scans from the start. Query parse is **O(N²)**; SIMD on top of a quadratic walk is a micro-optimisation on the quadratic term. A separate non-SIMD refactor (byte-indexed parser with ASCII fast path) is the right fix — tracked as follow-up |

Two decisions follow from this audit:

1. **RLE, CRC infra, and the JSON primitive** ship as landed.
2. **Record codec batch and tokenizer SIMD are dropped** from phase 3.
   The former has no room to improve; the latter needs the underlying
   O(N²) parser refactor before any SIMD layer would matter.

### RLE find-run-length — gated win

`simd::rle::find_run_length` replaces the misnamed "SIMD-accelerated"
(actually scalar) inner loop of `compress_simd_rle`. Measured on
Zen 4 scanning a full slice through the run-length finder:

| Shape    | Size     | Scalar  | Dispatch | Ratio  |
|----------|----------|---------|----------|--------|
| uniform  | 1 024    | 195 ns  | 64 ns    | 3.0× faster |
| uniform  | 16 384   | 3.2 µs  | 1.0 µs   | 3.2× faster |
| uniform  | 262 144  | 51 µs   | 18.5 µs  | 2.75× faster |
| grouped  | 1 024    | 331 ns  | 558 ns   | **0.59× — slower** |
| grouped  | 16 384   | 5.4 µs  | 9.0 µs   | 0.60× — slower |
| unique   | 16 384   | 8.8 µs  | 72 µs    | 0.12× — 8× slower |
| unique   | 262 144  | 137 µs  | 1.14 ms  | 0.12× — 8× slower |

The SIMD setup cost (broadcast + load + cmp + movemask + trailing_ones,
~5–7 cycles) beats scalar only when the run is long enough to
amortise it over ≥ 4 lanes. `unique` (runs of 1) and `grouped` (average
run ≈ 8.5) pay the setup for nothing.

**Production use is gated.** `choose_compression_type` only picks
`CompressionType::SimdRLE` when `repeat_ratio > 0.3`, i.e. more than
30% of consecutive pairs are equal. That filters out the `unique`
case entirely and pushes the workload into hub-node-style
distributions where runs are long — the regime where SIMD measurably
wins. Byte-identical output is enforced by
`tests/simd_rle_parity.rs` (7 cases including 256× proptest).

### CRC32C revisited

Decision: **WAL default stays on `crc32fast`.** The dual-format
infrastructure (magic byte + algo field) is still worth shipping
because:

- It enables per-frame algorithm tagging without breaking old files.
- When AVX-512 VPCLMULQDQ parallel CRC32C lands in a future kernel
  implementation, or on platforms where CRC32C has interop value
  (iSCSI / ZFS / cloud-storage object stores), the algo field lets
  us switch in one line — `append_with_algo(entry, Crc32C)` — with
  no migration work and no legacy-read breakage.

The `simd::crc32c` wrapper is kept as a public primitive for interop
use. The `Crc32C` variant of `ChecksumAlgo` is exercised end-to-end
by `wal::tests::v2_frame_with_crc32c_roundtrips`.

The ADR-003 acceptance target of ≥4× AVX2 vs scalar at dim=768 is
cleared by more than 3×. CI is expected to refuse a PR that regresses
dispatch by >5% on any scale already benchmarked — the check hook is
tracked in `phase3_rpc-protocol-docs-benchmarks` (the benches land
first, the gate wires up in that task).

## 9. Adding a new kernel — checklist

1. Pick the op (e.g. `sum_f64` for phase-2 aggregates).
2. Add `scalar::<op>(..)` with `#[inline(always)]`.
3. Add `x86::<op>_sse42(..)`, `<op>_avx2(..)`, `<op>_avx512(..)` — each
   `unsafe` and `#[target_feature(enable = "...")]`. Prefer 4
   independent accumulators for AVX2/AVX-512 FMA ops, native masked
   load for AVX-512 tails.
4. Add `aarch64::<op>_neon(..)` — `unsafe`, `#[target_feature(enable =
   "neon")]`.
5. Add `<op>` public wrapper in the appropriate dispatch file
   (`distance.rs` or `bitmap.rs`) using `OnceLock<unsafe fn>`. Every
   `unsafe {}` block must carry a `// SAFETY:` comment tying the call
   to the `cpu()` flag that guarantees the required `target_feature`.
6. Extend the matching `kernel_tiers()` to report the new op.
7. Add proptest parity tests that compare:
   - Dispatch vs scalar (cascade-independent sanity).
   - Direct SIMD-tier vs scalar under `prop_assume!(cpu().<tier>)` —
     one for each tier the op implements.
8. Add a Criterion bench entry in `simd_distance.rs` /
   `simd_popcount.rs` (or a new file with a `[[bench]]` entry in
   `nexus-core/Cargo.toml`).
9. Update the "Reference numbers" table in §8 after measuring on the
   reference machine.
10. Wire every call site in the production code to the new dispatch
    wrapper; delete any hand-rolled scalar loops that the dispatcher
    now covers.

## 10. Cross-compile notes

- AArch64 cross-compile requires a C cross-toolchain (`cc-rs` is a
  transitive dep of `tantivy` and `zstd`). When the local environment
  lacks it, `cargo check --target aarch64-unknown-linux-gnu` fails in
  a dependency build script before the NEON code is even compiled.
  The NEON intrinsics used (`vld1q_f32`, `vfmaq_f32`, `vaddvq_f32`,
  `vdupq_n_f32`, `vmulq_f32`, `vst1q_f32`, `vsubq_f32`, `vcntq_u8`,
  `vaddlvq_u8`, `vandq_u8`) are all stable in `std::arch::aarch64`
  and will be validated on the first ARM CI runner.
- Other architectures (riscv64, wasm32) compile with scalar fallback
  only; there is no `cfg`-gated module for them yet.

## 11. Files and tests of record

| Kind      | Path                                              |
|-----------|---------------------------------------------------|
| Module    | `nexus-core/src/simd/`                            |
| Unit tests| `nexus-core/src/simd/{dispatch,scalar,distance,bitmap,reduce,compare}.rs` under `#[cfg(test)]` |
| Proptest  | `nexus-core/tests/simd_scalar_properties.rs`      |
| Proptest  | `nexus-core/tests/simd_distance_parity.rs`        |
| Proptest  | `nexus-core/tests/simd_bitmap_parity.rs`          |
| Proptest  | `nexus-core/tests/simd_reduce_parity.rs`          |
| Proptest  | `nexus-core/tests/simd_compare_parity.rs`         |
| Benches   | `nexus-core/benches/simd_distance.rs`             |
| Benches   | `nexus-core/benches/simd_popcount.rs`             |
| Benches   | `nexus-core/benches/simd_reduce.rs`               |
| Benches   | `nexus-core/benches/simd_compare.rs`              |
| Decision  | `.rulebook/decisions/003-simd-dispatch-…`         |

Related spec updates will land in `knn-integration.md` (§ "Distance
implementation") and `wal-mvcc.md` (§ "CRC32C migration") as the
downstream call sites are replaced.
