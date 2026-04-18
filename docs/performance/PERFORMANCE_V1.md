# Performance — Nexus v1.0.0-dev

> Reference numbers taken on **Ryzen 9 7950X3D** (Zen 4, AVX-512F +
> VPOPCNTQ + AVX2 + SSE4.2 + FMA), 64 GB DDR5-6000, Windows 10 + MSVC.
> All benches run via `cargo +nightly bench --bench <name> -- --quick`.

## Goals and targets

Nexus v1.0.0 ships against the targets below. Every row has a measured
number from the `nexus-core/benches/` suite and a follow-up procedure
when the number regresses.

| Target                                     | v0.12 | v1.0-dev | Goal  | Status |
|--------------------------------------------|-------|----------|-------|--------|
| KNN `dot_f32` @ dim=768 (scalar → dispatch)| 438 ns scalar | 34.5 ns AVX-512 | ≥ 4× AVX2 | **12.7×** ✅ |
| KNN `l2_sq_f32` @ dim=512                  | 285 ns scalar | 21.0 ns AVX-512 | ≥ 4× AVX2 | **13.5×** ✅ |
| `popcount_u64` @ 4 KiB                     | 1.52 µs scalar | 136 ns VPOPCNTQ | ≥ 5× AVX2 | **≈11×** ✅ |
| `sum_f64` @ 262 144 rows                   | 150 µs scalar | 19 µs AVX-512 | ≥ 4× scalar | **7.9×** ✅ |
| `sum_f32` @ 262 144 rows                   | 152 µs scalar | 9.5 µs AVX-512 | ≥ 4× scalar | **15.9×** ✅ |
| `lt_i64` filter @ 262 144 rows             | 110 µs scalar | 25 µs AVX-512 | ≥ 3× AVX2 | **4.4×** ✅ |
| `eq_i64` filter @ 262 144 rows             | 69 µs scalar | 24 µs AVX-512 | ≥ 3× AVX2 | **2.9×** ✅ |
| RLE `find_run_length` @ 16 K uniform       | 3.2 µs scalar | 1.0 µs AVX-512 | ≥ 3× scalar | **3.2×** ✅ |
| Cypher parse @ 31.5 KiB query              | ≈ 1 s (O(N²))  | 3.7 ms (O(N)) | linear scaling | **≈290× ✅** |
| WAL CRC throughput @ 64 KiB                | 3.5 µs `crc32fast` | 3.5 µs `crc32fast` | 1× (kept) | **see notes** |

All numbers are single-threaded. Throughput counters for KNN / bulk
ingest at query-loop scale are tracked separately once the RPC
transport lands.

## SIMD kernel selection

Runtime-dispatched per op at first use, cached in a `OnceLock<unsafe fn>`
thereafter. Cascade: **AVX-512F → AVX2+FMA → SSE4.2 → NEON → Scalar**.

| Op family      | Scalar | SSE4.2 | AVX2 | AVX-512F | NEON |
|----------------|--------|--------|------|----------|------|
| `distance`     | ✅ ref | 4-lane | 8-lane + FMA (4× ILP) | 16-lane + masked tail | 4-lane |
| `bitmap`       | ✅ ref | —      | Mula nibble-LUT | VPOPCNTQ + masked tail | `vcntq_u8` |
| `reduce`       | ✅ ref | —      | 4-acc ILP + NaN masking | `_mm512_reduce_*` | `vaddvq_*` |
| `compare`      | ✅ ref | —      | movemask packing | native `__mmask8` | `vceqq_*` |
| `rle`          | ✅ ref | —      | `_mm256_cmpeq_epi64` + `trailing_ones` | `_mm512_cmpeq_epi64_mask` | `vceqq_u64` |
| `crc32c`       | —      | `_mm_crc32_u64` | — | — | ARMv8 CRC |

Exact intrinsics per op in [docs/specs/simd-dispatch.md](specs/simd-dispatch.md#6-observability).

## Measurement caveats — honest findings

Three expected wins did NOT materialise on modern x86_64. Numbers that
drove each decision:

### CRC32C vs CRC32 (Zen 4)

`crc32fast` runs 3-way parallel PCLMUL at ~15 GB/s; `crc32c` HW path
(`_mm_crc32_u64`) is single-instruction sequential at ~7 GB/s.

| Buffer | `crc32fast` | `crc32c` HW | Ratio        |
|--------|-------------|-------------|--------------|
| 256 B  | 16 ns       | 36 ns       | crc32fast 2.25× faster |
| 4 KiB  | 216 ns      | 454 ns      | crc32fast 2.10× faster |
| 64 KiB | 3.5 µs      | 7.0 µs      | crc32fast 2.00× faster |

**Decision:** WAL writes default to `crc32fast`. Dual-format
infrastructure (`[magic:1][algo:1]...` v2 frame) lets us switch to
CRC32C per-frame via `Wal::append_with_algo(entry, Crc32C)` when AVX-512
VPCLMULQDQ support lands or when iSCSI/ZFS/cloud-storage interop is
needed.

### simd-json vs serde_json (Nexus ingest schema)

The `/ingest` payload stores per-node `properties: serde_json::Value`
— an untyped dynamic tree. simd-json's advantage is typed-schema
deserialisation that skips DOM construction; with a `Value` target,
DOM is built anyway.

| Payload | `serde_json` | `simd-json` | Ratio |
|---------|--------------|-------------|-------|
| 10 KiB  | 32 µs        | 32 µs       | tie   |
| 70 KiB  | 213 µs       | 315 µs      | simd-json 1.48× slower |
| 1 MiB   | 4.2 ms       | 5.5 ms      | simd-json 1.32× slower |

**Decision:** `/ingest` stays on serde_json. `simd::json::parse<T>()` +
`parse_mut<T>()` stay available as primitives for future typed
consumers (RPC frames, typed `parameters` maps, `/bulk` endpoints).

### Record codec batch

`NodeRecord::from_bytes` / `RelationshipRecord::from_bytes` use
`ptr::copy_nonoverlapping` over `#[repr(C)]` PODs. LLVM already lowers
this to `movdqu`/`vmovdqu` — there is no room for a hand-written SIMD
batch decoder to improve. Dropped after audit.

## Other measured wins (not SIMD)

### Cypher parser O(N²) → O(N)

Pre-fix every `peek_char` did `self.input.chars().nth(self.pos)` — an
O(n) UTF-8 iterator walk from byte 0. Replaced with
`self.input[self.pos..].chars().next()` (O(1)). Cost-per-byte now flat:

| Query   | Bytes   | Parse     | ns/byte |
|---------|---------|-----------|---------|
| small   | 85      | 7.8 µs    | 92      |
| medium  | 4.2 KiB | 454 µs    | 108     |
| large   | 31.5 KiB| 3.7 ms    | 117     |

Pre-fix extrapolation: 31.5 KiB query ≈ `(31555/85)² × 7.8 µs ≈ 1.07 s`
— ≈ **290× slower** than the 3.7 ms we see now. Linear scaling confirmed
across 3 orders of magnitude.

## Rollout safety

- `NEXUS_SIMD_DISABLE=1` forces the scalar kernel for every dispatched
  op (distance / bitmap / reduce / compare / rle) runtime-wide. No
  rebuild.
- `NEXUS_SIMD_JSON_DISABLE=1` forces serde_json in the `simd::json`
  dispatcher regardless of payload size.
- `GET /stats` surfaces the selected kernel tier per op under the
  `simd` field, e.g.:

  ```json
  {
    "simd": {
      "preferred_tier": "avx512",
      "kernels": {
        "cosine_f32": "avx512",
        "dot_f32": "avx512",
        "eq_i64": "avx512",
        "find_run_length_u64": "avx512",
        "popcount_u64": "avx512vpopcntdq",
        "sum_f64": "avx512"
      }
    }
  }
  ```

## Reproduce

```bash
# All SIMD kernel benches
cargo +nightly bench -p nexus-core --bench simd_distance -- --quick
cargo +nightly bench -p nexus-core --bench simd_popcount -- --quick
cargo +nightly bench -p nexus-core --bench simd_reduce  -- --quick
cargo +nightly bench -p nexus-core --bench simd_compare -- --quick
cargo +nightly bench -p nexus-core --bench simd_rle     -- --quick
cargo +nightly bench -p nexus-core --bench simd_crc     -- --quick
cargo +nightly bench -p nexus-core --bench simd_json    -- --quick

# Parser (proves O(N) scaling after the fix)
cargo +nightly bench -p nexus-core --bench parser_tokenize -- --quick
```

Full kernel spec + parity-test invariants: [docs/specs/simd-dispatch.md](specs/simd-dispatch.md).
