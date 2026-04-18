# Proposal: phase1_simd-knn-distance-kernels

## Why

Nexus markets itself as a graph DB with **native vector search** (HNSW via
`hnsw_rs`, KNN queries target <2 ms p95). Yet every f32 arithmetic kernel
exercised on the critical path today is **pure scalar Rust**:

- `nexus-core/src/index/mod.rs:1054` `cosine_similarity` — naive
  `iter().zip().map().sum()` loop, runs for every reranking pass.
- `nexus-core/src/index/mod.rs` `normalize_vector` — scalar sqrt + div,
  runs on every HNSW insert.
- `nexus-core/src/graph/algorithms/traversal.rs:1607` node-level
  cosine/Jaccard over neighbour bitsets — converts to f64 and folds
  scalar, completely ignoring the hardware popcount unit.

At dim=768 (typical BERT/MiniLM embedding) that is **768 scalar f32
multiplies + 767 scalar adds per distance**. Every comparable engine
(FAISS, ScaNN, pgvector, even `hnsw_rs` itself when built with
`stdsimd`) runs those loops through SSE/AVX/NEON and sees **4–8x
speedups**. We are leaving that on the table.

This phase establishes the SIMD infrastructure for the whole codebase
AND lands the highest-yield kernel (vector distances), so that phases 2
and 3 only have to plug into the dispatch layer that already exists.

## What Changes

### 1. New `nexus-core::simd` module — infrastructure

```
nexus-core/src/simd/
|- mod.rs           # public facade, runtime feature detection
|- dispatch.rs      # CpuFeatures, pick_kernel<F>() helpers
|- scalar.rs        # reference implementations (always compiled)
|- x86.rs           # SSE2/SSE4.2/AVX2/AVX-512 kernels (cfg(x86_64))
|- aarch64.rs       # NEON kernels (cfg(aarch64))
|- portable.rs      # std::simd fallback (cfg(feature = "simd-portable"))
|- distance/
|   |- mod.rs       # pub fn dot, cosine, l2, normalize
|   |- x86_avx2.rs
|   |- x86_avx512.rs
|   |- aarch64_neon.rs
|   |- scalar.rs
|- bitmap.rs        # popcount, and/or/xor + count — used in phase 2
```

### 2. Runtime CPU dispatch

```rust
pub struct CpuFeatures {
    pub avx512f: bool,
    pub avx512vpopcntdq: bool,
    pub avx2: bool,
    pub sse42: bool,
    pub neon: bool,
}

pub static CPU: OnceLock<CpuFeatures> = OnceLock::new();

pub fn cpu() -> &'static CpuFeatures { CPU.get_or_init(CpuFeatures::detect) }
```

Detection uses `std::is_x86_feature_detected!` on x86_64 and
`std::arch::is_aarch64_feature_detected!` on aarch64. Detection runs
**once** at first call; every subsequent hot-path call reads an atomic
bool. No per-call detection overhead.

Kernel pointers are resolved lazily per routine:

```rust
type F32DotFn = unsafe fn(&[f32], &[f32]) -> f32;

static DOT_KERNEL: OnceLock<F32DotFn> = OnceLock::new();

pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    let kernel = DOT_KERNEL.get_or_init(pick_dot_kernel);
    // SAFETY: kernel was selected based on runtime-detected features
    unsafe { kernel(a, b) }
}
```

### 3. Distance kernels (dim-agnostic, tail-safe)

| Op              | Scalar | SSE4.2 (4 lanes) | AVX2 (8 lanes) | AVX-512 (16 lanes) | NEON (4 lanes) | Portable |
|-----------------|--------|-------------------|-----------------|--------------------|-----------------|----------|
| `dot(a, b)`     | ✅      | ✅ `_mm_mul_ps`+`_mm_hadd_ps` | ✅ `_mm256_fmadd_ps` | ✅ `_mm512_fmadd_ps` + `_mm512_reduce_add_ps` | ✅ `vfmaq_f32`+`vaddvq_f32` | ✅ `f32x8` |
| `l2_sq(a, b)`   | ✅      | ✅ sub+mul+add    | ✅ FMA(sub,sub) | ✅ VFMADD          | ✅ `vsubq_f32`+FMA | ✅ |
| `cosine(a, b)`  | ✅ (uses dot + norms) | ✅ | ✅ | ✅ | ✅ | ✅ |
| `normalize(&mut)` | ✅    | ✅ sum_squares+rsqrt | ✅ rsqrt         | ✅ VRSQRT14PS     | ✅ `vrsqrteq_f32` (+Newton) | ✅ |

Tail handling: process `n / LANES * LANES` elements in SIMD, then a
masked tail. AVX-512 uses `_mm512_maskz_loadu_ps` directly. For AVX2/
SSE/NEON we fall back to scalar for the tail — empirically the tail is
<16 elements so this is a 4–8 cycle overhead regardless of dimension.

### 4. Replace call sites

- `nexus-core/src/index/mod.rs` — `cosine_similarity` (inline and test
  helper) and `normalize_vector` call the new dispatch functions.
- `nexus-core/src/graph/algorithms/traversal.rs:1607` node-cosine over
  bitsets reworked to use `simd::bitmap::popcount` and
  `simd::bitmap::and_popcount` instead of f64 emulation.
- `hnsw_rs` distance trait impl: provide our own `DistCosine`/`DistL2`
  wrapping the SIMD kernels so HNSW construction and search benefit
  too.

### 5. Cargo features

```toml
[features]
default = ["simd"]
simd = []                  # always-on runtime dispatch
simd-avx512 = ["simd"]     # compile AVX-512 intrinsics (else dead code)
simd-portable = ["simd"]   # use std::simd (nightly)
```

Build still works on any stable Rust — `simd-portable` is opt-in.
`cargo +nightly build --release` is the project default per CLAUDE.md,
so AVX-512 + portable-SIMD are compiled by default on our dev
machines; the release binary still runs on any SSE2 x86_64 because
kernel selection is dynamic.

### 6. Cross-arch support matrix

| Arch         | Baseline              | Detected at runtime              |
|--------------|-----------------------|----------------------------------|
| x86_64       | SSE2 (always)         | SSE4.2, AVX, AVX2, AVX-512F/VL/BW, VPOPCNTQ |
| aarch64      | NEON (always in ARMv8)| SVE2 (stub; not used in phase 1) |
| RISC-V 64    | —                     | V extension (stub; scalar only)  |
| wasm32       | —                     | `simd128` via `cfg(target_feature)` (opt-in, phase 3) |

Unsupported architectures silently use the scalar kernel — no compile
errors, no runtime panics.

### 7. Bench gates and correctness

- New Criterion suite `nexus-core/benches/simd_distance.rs` with:
  - dim ∈ {32, 128, 256, 512, 768, 1024, 1536}
  - batch sizes 1, 10, 100, 1000
  - 3 scenarios per dim: `dot`, `cosine`, `l2_sq`, `normalize`
- `proptest` property tests for **every kernel vs scalar** asserting
  `abs_diff <= 1e-5 * max(abs(result), 1.0)`.
- CI check: regression >5% vs baseline on the reference machine fails
  the job.
- Existing 300/300 Neo4j compat tests must remain green (no numerical
  drift from reordered fma).

## Impact

- **Affected specs**: new `docs/specs/simd-dispatch.md` documenting the
  runtime-detection architecture and how to add a new kernel.
- **Affected code**:
  - NEW: `nexus-core/src/simd/*` (≈1800 LOC including tests, 12 files)
  - NEW: `nexus-core/benches/simd_distance.rs` (≈400 LOC)
  - MODIFIED: `nexus-core/src/index/mod.rs` (replace cosine/normalize
    call sites; no behaviour change)
  - MODIFIED: `nexus-core/src/graph/algorithms/traversal.rs`
    (node-cosine via bitset popcount kernels)
  - MODIFIED: `nexus-core/Cargo.toml` (+ `bytemuck`, `aligned-vec`,
    feature flags)
- **Breaking change**: NO — all public APIs unchanged; kernel selection
  is transparent.
- **User benefit**:
  - **4–8x** faster KNN queries at dim=768.
  - **3–4x** faster HNSW index build (normalize on insert).
  - **5–10x** faster graph-level cosine/Jaccard on dense adjacency.
  - Build-once-run-anywhere: the same release binary uses AVX-512 on a
    Zen 4 and SSE2 on an old Xeon without rebuild.

## Non-goals

- Distance kernels in f16/bf16 — phase 4, depends on AVX-512FP16 or
  SVE2 BF16 being common enough to justify kernels.
- Quantised distances (int8, binary) — separate phase aligned with a
  storage redesign.
- Vectorising the HNSW graph traversal itself — `hnsw_rs` is external;
  we just hand it faster distance functions.
- GPU kernels — out of scope forever in `nexus-core`; separate
  `nexus-gpu` crate if ever needed.

## Reference

- FAISS L2/IP distance primitives: `faiss/impl/platform_macros.h`
- `simsimd` crate (reference implementation of f32 dot/cos/L2 with the
  same dispatch model we are copying here): <https://github.com/ashvardanian/SimSIMD>
- `pulp` / `wide` crates for portable-SIMD ergonomics on stable Rust.
