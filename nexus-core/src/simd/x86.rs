//! x86_64 SIMD kernels (SSE4.2 / AVX2 / AVX-512).
//!
//! Every public function in this module is `unsafe` because calling it
//! on a CPU that does not advertise the corresponding feature is
//! undefined behaviour. The safe entry points in `simd::distance` and
//! `simd::bitmap` guarantee the invariant by consulting the runtime
//! `CpuFeatures` probe (see `dispatch.rs`).
//!
//! The layout mirrors `scalar.rs` one-to-one so the dispatch layer can
//! store function pointers without adaptation.

#![cfg(target_arch = "x86_64")]

use std::arch::x86_64::*;

// ── f32 dot product ───────────────────────────────────────────────────────────

/// Dot product, SSE4.2 path (4 lanes of f32).
///
/// # Safety
/// The host CPU MUST advertise `sse4.2`. Callers acquire this guarantee
/// via `dispatch::cpu().sse42`.
#[target_feature(enable = "sse4.2")]
pub unsafe fn dot_f32_sse42(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let chunks = len / 4;
    let mut acc = _mm_setzero_ps();
    let ap = a.as_ptr();
    let bp = b.as_ptr();

    for i in 0..chunks {
        let av = _mm_loadu_ps(ap.add(i * 4));
        let bv = _mm_loadu_ps(bp.add(i * 4));
        acc = _mm_add_ps(acc, _mm_mul_ps(av, bv));
    }

    // Horizontal reduction: [a+b, c+d, a+b, c+d] -> [a+b+c+d, ...]
    let sum = _mm_hadd_ps(acc, acc);
    let sum = _mm_hadd_ps(sum, sum);
    let mut result = _mm_cvtss_f32(sum);

    for i in (chunks * 4)..len {
        result += *ap.add(i) * *bp.add(i);
    }
    result
}

/// Dot product, AVX2+FMA path (8 lanes of f32 with 4 independent
/// accumulators to hide FMA latency).
///
/// # Safety
/// The host CPU MUST advertise `avx2` and `fma`.
#[target_feature(enable = "avx2,fma")]
pub unsafe fn dot_f32_avx2(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let ap = a.as_ptr();
    let bp = b.as_ptr();

    let mut acc0 = _mm256_setzero_ps();
    let mut acc1 = _mm256_setzero_ps();
    let mut acc2 = _mm256_setzero_ps();
    let mut acc3 = _mm256_setzero_ps();

    let unroll_chunks = len / 32;
    for i in 0..unroll_chunks {
        let base = i * 32;
        let a0 = _mm256_loadu_ps(ap.add(base));
        let b0 = _mm256_loadu_ps(bp.add(base));
        acc0 = _mm256_fmadd_ps(a0, b0, acc0);
        let a1 = _mm256_loadu_ps(ap.add(base + 8));
        let b1 = _mm256_loadu_ps(bp.add(base + 8));
        acc1 = _mm256_fmadd_ps(a1, b1, acc1);
        let a2 = _mm256_loadu_ps(ap.add(base + 16));
        let b2 = _mm256_loadu_ps(bp.add(base + 16));
        acc2 = _mm256_fmadd_ps(a2, b2, acc2);
        let a3 = _mm256_loadu_ps(ap.add(base + 24));
        let b3 = _mm256_loadu_ps(bp.add(base + 24));
        acc3 = _mm256_fmadd_ps(a3, b3, acc3);
    }
    let acc = _mm256_add_ps(_mm256_add_ps(acc0, acc1), _mm256_add_ps(acc2, acc3));

    // Continue with 8-lane blocks for the remainder >= 8.
    let mut acc = acc;
    let mut i = unroll_chunks * 32;
    while i + 8 <= len {
        let av = _mm256_loadu_ps(ap.add(i));
        let bv = _mm256_loadu_ps(bp.add(i));
        acc = _mm256_fmadd_ps(av, bv, acc);
        i += 8;
    }

    // Horizontal reduction of the 256-bit accumulator.
    let low = _mm256_castps256_ps128(acc);
    let high = _mm256_extractf128_ps::<1>(acc);
    let sum128 = _mm_add_ps(low, high);
    let sum64 = _mm_add_ps(sum128, _mm_movehl_ps(sum128, sum128));
    let sum32 = _mm_add_ss(sum64, _mm_shuffle_ps::<0b01>(sum64, sum64));
    let mut result = _mm_cvtss_f32(sum32);

    // Scalar tail.
    while i < len {
        result += *ap.add(i) * *bp.add(i);
        i += 1;
    }
    result
}

/// Dot product, AVX-512F path (16 lanes of f32, masked tail load).
///
/// # Safety
/// The host CPU MUST advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn dot_f32_avx512(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let ap = a.as_ptr();
    let bp = b.as_ptr();

    let mut acc = _mm512_setzero_ps();
    let chunks = len / 16;
    for i in 0..chunks {
        let av = _mm512_loadu_ps(ap.add(i * 16));
        let bv = _mm512_loadu_ps(bp.add(i * 16));
        acc = _mm512_fmadd_ps(av, bv, acc);
    }
    let tail = len - chunks * 16;
    if tail > 0 {
        let mask: __mmask16 = (1u16 << tail) - 1;
        let av = _mm512_maskz_loadu_ps(mask, ap.add(chunks * 16));
        let bv = _mm512_maskz_loadu_ps(mask, bp.add(chunks * 16));
        acc = _mm512_fmadd_ps(av, bv, acc);
    }
    _mm512_reduce_add_ps(acc)
}

// ── f32 squared L2 distance ───────────────────────────────────────────────────

/// # Safety
/// Host MUST advertise `sse4.2`.
#[target_feature(enable = "sse4.2")]
pub unsafe fn l2_sq_f32_sse42(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let chunks = len / 4;
    let mut acc = _mm_setzero_ps();
    let ap = a.as_ptr();
    let bp = b.as_ptr();

    for i in 0..chunks {
        let av = _mm_loadu_ps(ap.add(i * 4));
        let bv = _mm_loadu_ps(bp.add(i * 4));
        let d = _mm_sub_ps(av, bv);
        acc = _mm_add_ps(acc, _mm_mul_ps(d, d));
    }

    let sum = _mm_hadd_ps(acc, acc);
    let sum = _mm_hadd_ps(sum, sum);
    let mut result = _mm_cvtss_f32(sum);

    for i in (chunks * 4)..len {
        let d = *ap.add(i) - *bp.add(i);
        result += d * d;
    }
    result
}

/// # Safety
/// Host MUST advertise `avx2` and `fma`.
#[target_feature(enable = "avx2,fma")]
pub unsafe fn l2_sq_f32_avx2(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let ap = a.as_ptr();
    let bp = b.as_ptr();

    let mut acc0 = _mm256_setzero_ps();
    let mut acc1 = _mm256_setzero_ps();
    let mut acc2 = _mm256_setzero_ps();
    let mut acc3 = _mm256_setzero_ps();

    let unroll_chunks = len / 32;
    for i in 0..unroll_chunks {
        let base = i * 32;
        let d0 = _mm256_sub_ps(_mm256_loadu_ps(ap.add(base)), _mm256_loadu_ps(bp.add(base)));
        acc0 = _mm256_fmadd_ps(d0, d0, acc0);
        let d1 = _mm256_sub_ps(
            _mm256_loadu_ps(ap.add(base + 8)),
            _mm256_loadu_ps(bp.add(base + 8)),
        );
        acc1 = _mm256_fmadd_ps(d1, d1, acc1);
        let d2 = _mm256_sub_ps(
            _mm256_loadu_ps(ap.add(base + 16)),
            _mm256_loadu_ps(bp.add(base + 16)),
        );
        acc2 = _mm256_fmadd_ps(d2, d2, acc2);
        let d3 = _mm256_sub_ps(
            _mm256_loadu_ps(ap.add(base + 24)),
            _mm256_loadu_ps(bp.add(base + 24)),
        );
        acc3 = _mm256_fmadd_ps(d3, d3, acc3);
    }
    let mut acc = _mm256_add_ps(_mm256_add_ps(acc0, acc1), _mm256_add_ps(acc2, acc3));

    let mut i = unroll_chunks * 32;
    while i + 8 <= len {
        let d = _mm256_sub_ps(_mm256_loadu_ps(ap.add(i)), _mm256_loadu_ps(bp.add(i)));
        acc = _mm256_fmadd_ps(d, d, acc);
        i += 8;
    }

    let low = _mm256_castps256_ps128(acc);
    let high = _mm256_extractf128_ps::<1>(acc);
    let sum128 = _mm_add_ps(low, high);
    let sum64 = _mm_add_ps(sum128, _mm_movehl_ps(sum128, sum128));
    let sum32 = _mm_add_ss(sum64, _mm_shuffle_ps::<0b01>(sum64, sum64));
    let mut result = _mm_cvtss_f32(sum32);

    while i < len {
        let d = *ap.add(i) - *bp.add(i);
        result += d * d;
        i += 1;
    }
    result
}

/// # Safety
/// Host MUST advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn l2_sq_f32_avx512(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let ap = a.as_ptr();
    let bp = b.as_ptr();

    let mut acc = _mm512_setzero_ps();
    let chunks = len / 16;
    for i in 0..chunks {
        let d = _mm512_sub_ps(
            _mm512_loadu_ps(ap.add(i * 16)),
            _mm512_loadu_ps(bp.add(i * 16)),
        );
        acc = _mm512_fmadd_ps(d, d, acc);
    }
    let tail = len - chunks * 16;
    if tail > 0 {
        let mask: __mmask16 = (1u16 << tail) - 1;
        let av = _mm512_maskz_loadu_ps(mask, ap.add(chunks * 16));
        let bv = _mm512_maskz_loadu_ps(mask, bp.add(chunks * 16));
        let d = _mm512_sub_ps(av, bv);
        acc = _mm512_fmadd_ps(d, d, acc);
    }
    _mm512_reduce_add_ps(acc)
}

// ── f32 cosine distance ───────────────────────────────────────────────────────
//
// Cosine reuses the dot-product kernel to compute `<a,b>`, `<a,a>`,
// `<b,b>` in three passes. Tempting to fuse into one pass, but the
// three-accumulator version is less lane-heavy (especially on AVX-512
// where extra independent accumulators offer diminishing returns) and
// keeps the code symmetric with the scalar reference.

#[target_feature(enable = "sse4.2")]
pub unsafe fn cosine_f32_sse42(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_f32_sse42(a, b);
    let na = dot_f32_sse42(a, a);
    let nb = dot_f32_sse42(b, b);
    let denom = (na * nb).sqrt();
    if denom == 0.0 { 0.0 } else { 1.0 - dot / denom }
}

#[target_feature(enable = "avx2,fma")]
pub unsafe fn cosine_f32_avx2(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_f32_avx2(a, b);
    let na = dot_f32_avx2(a, a);
    let nb = dot_f32_avx2(b, b);
    let denom = (na * nb).sqrt();
    if denom == 0.0 { 0.0 } else { 1.0 - dot / denom }
}

#[target_feature(enable = "avx512f")]
pub unsafe fn cosine_f32_avx512(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_f32_avx512(a, b);
    let na = dot_f32_avx512(a, a);
    let nb = dot_f32_avx512(b, b);
    let denom = (na * nb).sqrt();
    if denom == 0.0 { 0.0 } else { 1.0 - dot / denom }
}

// ── f32 normalize in place ────────────────────────────────────────────────────
//
// Returns the pre-normalisation norm so the caller can reuse it.

#[target_feature(enable = "sse4.2")]
pub unsafe fn normalize_f32_sse42(v: &mut [f32]) -> f32 {
    let sq = dot_f32_sse42(v, v);
    let norm = sq.sqrt();
    if norm > 0.0 {
        let inv = 1.0 / norm;
        let len = v.len();
        let chunks = len / 4;
        let inv_v = _mm_set1_ps(inv);
        let vp = v.as_mut_ptr();
        for i in 0..chunks {
            let p = vp.add(i * 4);
            _mm_storeu_ps(p, _mm_mul_ps(_mm_loadu_ps(p), inv_v));
        }
        for i in (chunks * 4)..len {
            *vp.add(i) *= inv;
        }
    }
    norm
}

#[target_feature(enable = "avx2,fma")]
pub unsafe fn normalize_f32_avx2(v: &mut [f32]) -> f32 {
    let sq = dot_f32_avx2(v, v);
    let norm = sq.sqrt();
    if norm > 0.0 {
        let inv = 1.0 / norm;
        let len = v.len();
        let chunks = len / 8;
        let inv_v = _mm256_set1_ps(inv);
        let vp = v.as_mut_ptr();
        for i in 0..chunks {
            let p = vp.add(i * 8);
            _mm256_storeu_ps(p, _mm256_mul_ps(_mm256_loadu_ps(p), inv_v));
        }
        for i in (chunks * 8)..len {
            *vp.add(i) *= inv;
        }
    }
    norm
}

#[target_feature(enable = "avx512f")]
pub unsafe fn normalize_f32_avx512(v: &mut [f32]) -> f32 {
    let sq = dot_f32_avx512(v, v);
    let norm = sq.sqrt();
    if norm > 0.0 {
        let inv = 1.0 / norm;
        let len = v.len();
        let chunks = len / 16;
        let inv_v = _mm512_set1_ps(inv);
        let vp = v.as_mut_ptr();
        for i in 0..chunks {
            let p = vp.add(i * 16);
            _mm512_storeu_ps(p, _mm512_mul_ps(_mm512_loadu_ps(p), inv_v));
        }
        let tail = len - chunks * 16;
        if tail > 0 {
            let mask: __mmask16 = (1u16 << tail) - 1;
            let p = vp.add(chunks * 16);
            let loaded = _mm512_maskz_loadu_ps(mask, p);
            let scaled = _mm512_mul_ps(loaded, inv_v);
            _mm512_mask_storeu_ps(p, mask, scaled);
        }
    }
    norm
}
