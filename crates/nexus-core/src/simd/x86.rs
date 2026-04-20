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

// ── i64 / f64 / f32 reductions — sum ──────────────────────────────────────────

/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn sum_i64_avx2(values: &[i64]) -> i64 {
    let len = values.len();
    let base_ptr = values.as_ptr();
    let mut acc0 = _mm256_setzero_si256();
    let mut acc1 = _mm256_setzero_si256();

    // Unroll 2: 8 × i64 per iteration.
    let unroll = len / 8;
    for i in 0..unroll {
        let p = base_ptr.add(i * 8) as *const __m256i;
        acc0 = _mm256_add_epi64(acc0, _mm256_loadu_si256(p));
        acc1 = _mm256_add_epi64(acc1, _mm256_loadu_si256(p.add(1)));
    }
    let mut acc = _mm256_add_epi64(acc0, acc1);

    // 4 × i64 single-lane remainder.
    let mut i = unroll * 8;
    while i + 4 <= len {
        acc = _mm256_add_epi64(acc, _mm256_loadu_si256(base_ptr.add(i) as *const __m256i));
        i += 4;
    }
    // Horizontal reduce 4 × i64.
    let low = _mm256_castsi256_si128(acc);
    let high = _mm256_extracti128_si256::<1>(acc);
    let sum128 = _mm_add_epi64(low, high);
    let shifted = _mm_shuffle_epi32::<0b11_10>(sum128);
    let total = _mm_add_epi64(sum128, shifted);
    let mut result = _mm_cvtsi128_si64(total);
    while i < len {
        result = result.wrapping_add(values[i]);
        i += 1;
    }
    result
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn sum_i64_avx512(values: &[i64]) -> i64 {
    let len = values.len();
    let ptr = values.as_ptr() as *const __m512i;
    let mut acc = _mm512_setzero_si512();
    let chunks = len / 8;
    for i in 0..chunks {
        acc = _mm512_add_epi64(acc, _mm512_loadu_si512(ptr.add(i)));
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let mask: __mmask8 = (1u8 << tail) - 1;
        let loaded = _mm512_maskz_loadu_epi64(mask, values.as_ptr().add(chunks * 8));
        acc = _mm512_add_epi64(acc, loaded);
    }
    _mm512_reduce_add_epi64(acc)
}

/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn sum_f64_avx2(values: &[f64]) -> f64 {
    let len = values.len();
    let ptr = values.as_ptr();
    let mut acc0 = _mm256_setzero_pd();
    let mut acc1 = _mm256_setzero_pd();
    let mut acc2 = _mm256_setzero_pd();
    let mut acc3 = _mm256_setzero_pd();
    let unroll = len / 16;
    for i in 0..unroll {
        let base = i * 16;
        acc0 = _mm256_add_pd(acc0, _mm256_loadu_pd(ptr.add(base)));
        acc1 = _mm256_add_pd(acc1, _mm256_loadu_pd(ptr.add(base + 4)));
        acc2 = _mm256_add_pd(acc2, _mm256_loadu_pd(ptr.add(base + 8)));
        acc3 = _mm256_add_pd(acc3, _mm256_loadu_pd(ptr.add(base + 12)));
    }
    let mut acc = _mm256_add_pd(_mm256_add_pd(acc0, acc1), _mm256_add_pd(acc2, acc3));
    let mut i = unroll * 16;
    while i + 4 <= len {
        acc = _mm256_add_pd(acc, _mm256_loadu_pd(ptr.add(i)));
        i += 4;
    }
    // Horizontal reduce.
    let low = _mm256_castpd256_pd128(acc);
    let high = _mm256_extractf128_pd::<1>(acc);
    let sum128 = _mm_add_pd(low, high);
    let shifted = _mm_unpackhi_pd(sum128, sum128);
    let sum = _mm_add_sd(sum128, shifted);
    let mut result = _mm_cvtsd_f64(sum);
    while i < len {
        result += values[i];
        i += 1;
    }
    result
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn sum_f64_avx512(values: &[f64]) -> f64 {
    let len = values.len();
    let ptr = values.as_ptr();
    let mut acc = _mm512_setzero_pd();
    let chunks = len / 8;
    for i in 0..chunks {
        acc = _mm512_add_pd(acc, _mm512_loadu_pd(ptr.add(i * 8)));
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let mask: __mmask8 = (1u8 << tail) - 1;
        let loaded = _mm512_maskz_loadu_pd(mask, ptr.add(chunks * 8));
        acc = _mm512_add_pd(acc, loaded);
    }
    _mm512_reduce_add_pd(acc)
}

/// # Safety
/// Host must advertise `avx2` and `fma`.
#[target_feature(enable = "avx2,fma")]
pub unsafe fn sum_f32_avx2(values: &[f32]) -> f32 {
    let len = values.len();
    let ptr = values.as_ptr();
    let mut acc0 = _mm256_setzero_ps();
    let mut acc1 = _mm256_setzero_ps();
    let mut acc2 = _mm256_setzero_ps();
    let mut acc3 = _mm256_setzero_ps();
    let unroll = len / 32;
    for i in 0..unroll {
        let base = i * 32;
        acc0 = _mm256_add_ps(acc0, _mm256_loadu_ps(ptr.add(base)));
        acc1 = _mm256_add_ps(acc1, _mm256_loadu_ps(ptr.add(base + 8)));
        acc2 = _mm256_add_ps(acc2, _mm256_loadu_ps(ptr.add(base + 16)));
        acc3 = _mm256_add_ps(acc3, _mm256_loadu_ps(ptr.add(base + 24)));
    }
    let mut acc = _mm256_add_ps(_mm256_add_ps(acc0, acc1), _mm256_add_ps(acc2, acc3));
    let mut i = unroll * 32;
    while i + 8 <= len {
        acc = _mm256_add_ps(acc, _mm256_loadu_ps(ptr.add(i)));
        i += 8;
    }
    let low = _mm256_castps256_ps128(acc);
    let high = _mm256_extractf128_ps::<1>(acc);
    let sum128 = _mm_add_ps(low, high);
    let sum64 = _mm_add_ps(sum128, _mm_movehl_ps(sum128, sum128));
    let sum32 = _mm_add_ss(sum64, _mm_shuffle_ps::<0b01>(sum64, sum64));
    let mut result = _mm_cvtss_f32(sum32);
    while i < len {
        result += values[i];
        i += 1;
    }
    result
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn sum_f32_avx512(values: &[f32]) -> f32 {
    let len = values.len();
    let ptr = values.as_ptr();
    let mut acc = _mm512_setzero_ps();
    let chunks = len / 16;
    for i in 0..chunks {
        acc = _mm512_add_ps(acc, _mm512_loadu_ps(ptr.add(i * 16)));
    }
    let tail = len - chunks * 16;
    if tail > 0 {
        let mask: __mmask16 = (1u16 << tail) - 1;
        let loaded = _mm512_maskz_loadu_ps(mask, ptr.add(chunks * 16));
        acc = _mm512_add_ps(acc, loaded);
    }
    _mm512_reduce_add_ps(acc)
}

// ── min / max reductions — f64, f32, NaN-skipping ─────────────────────────────
//
// Scalar reference skips NaN operands; an empty or all-NaN slice
// returns None. SIMD paths replace NaN lanes with the "neutral" end
// of the domain (+inf for min, -inf for max) before reducing, and
// track whether any real value was seen via a mask/accumulator.

/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn min_f64_avx2(values: &[f64]) -> Option<f64> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    let ptr = values.as_ptr();
    let pos_inf = _mm256_set1_pd(f64::INFINITY);
    let mut acc = pos_inf;
    let mut saw_real = false;

    let chunks = len / 4;
    for i in 0..chunks {
        let v = _mm256_loadu_pd(ptr.add(i * 4));
        // NaN test: v != v → mask bits set for NaN lanes.
        let nan_mask = _mm256_cmp_pd::<_CMP_UNORD_Q>(v, v);
        let safe = _mm256_blendv_pd(v, pos_inf, nan_mask);
        acc = _mm256_min_pd(acc, safe);
        let real_mask = _mm256_andnot_pd(nan_mask, _mm256_cmp_pd::<_CMP_EQ_OQ>(v, v));
        if _mm256_movemask_pd(real_mask) != 0 {
            saw_real = true;
        }
    }

    // Horizontal reduce the 4 × f64 accumulator.
    let low = _mm256_castpd256_pd128(acc);
    let high = _mm256_extractf128_pd::<1>(acc);
    let min128 = _mm_min_pd(low, high);
    let shifted = _mm_unpackhi_pd(min128, min128);
    let reduced = _mm_min_sd(min128, shifted);
    let mut result = _mm_cvtsd_f64(reduced);

    for i in (chunks * 4)..len {
        let v = values[i];
        if !v.is_nan() {
            if !saw_real || v < result {
                result = v;
            }
            saw_real = true;
        }
    }

    if saw_real { Some(result) } else { None }
}

/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn max_f64_avx2(values: &[f64]) -> Option<f64> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    let ptr = values.as_ptr();
    let neg_inf = _mm256_set1_pd(f64::NEG_INFINITY);
    let mut acc = neg_inf;
    let mut saw_real = false;

    let chunks = len / 4;
    for i in 0..chunks {
        let v = _mm256_loadu_pd(ptr.add(i * 4));
        let nan_mask = _mm256_cmp_pd::<_CMP_UNORD_Q>(v, v);
        let safe = _mm256_blendv_pd(v, neg_inf, nan_mask);
        acc = _mm256_max_pd(acc, safe);
        let real_mask = _mm256_andnot_pd(nan_mask, _mm256_cmp_pd::<_CMP_EQ_OQ>(v, v));
        if _mm256_movemask_pd(real_mask) != 0 {
            saw_real = true;
        }
    }

    let low = _mm256_castpd256_pd128(acc);
    let high = _mm256_extractf128_pd::<1>(acc);
    let max128 = _mm_max_pd(low, high);
    let shifted = _mm_unpackhi_pd(max128, max128);
    let reduced = _mm_max_sd(max128, shifted);
    let mut result = _mm_cvtsd_f64(reduced);

    for i in (chunks * 4)..len {
        let v = values[i];
        if !v.is_nan() {
            if !saw_real || v > result {
                result = v;
            }
            saw_real = true;
        }
    }

    if saw_real { Some(result) } else { None }
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn min_f64_avx512(values: &[f64]) -> Option<f64> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    let ptr = values.as_ptr();
    let pos_inf = _mm512_set1_pd(f64::INFINITY);
    let mut acc = pos_inf;
    let mut saw_real: __mmask8 = 0;

    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm512_loadu_pd(ptr.add(i * 8));
        let nan_mask = _mm512_cmp_pd_mask::<_CMP_UNORD_Q>(v, v);
        // mask_blend(mask, a, b): lane of b if mask bit 1, lane of a if 0.
        let safe = _mm512_mask_blend_pd(nan_mask, v, pos_inf);
        acc = _mm512_min_pd(acc, safe);
        saw_real |= !nan_mask;
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let load_mask: __mmask8 = (1u8 << tail) - 1;
        let v = _mm512_maskz_loadu_pd(load_mask, ptr.add(chunks * 8));
        let nan_mask = _mm512_cmp_pd_mask::<_CMP_UNORD_Q>(v, v);
        // NaN outside `load_mask` would also register; clamp to the
        // loaded lanes via bit-and so tail lanes that were zeroed by
        // maskz_load (and therefore pass NaN test as equal) don't
        // poison `saw_real`.
        let real_mask = load_mask & !nan_mask;
        // Replace NaN lanes AND out-of-range lanes with +inf before
        // reducing so the accumulator is not disturbed.
        let safe = _mm512_mask_blend_pd(!real_mask, v, pos_inf);
        acc = _mm512_min_pd(acc, safe);
        saw_real |= real_mask;
    }

    if saw_real == 0 {
        return None;
    }
    Some(_mm512_reduce_min_pd(acc))
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn max_f64_avx512(values: &[f64]) -> Option<f64> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    let ptr = values.as_ptr();
    let neg_inf = _mm512_set1_pd(f64::NEG_INFINITY);
    let mut acc = neg_inf;
    let mut saw_real: __mmask8 = 0;

    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm512_loadu_pd(ptr.add(i * 8));
        let nan_mask = _mm512_cmp_pd_mask::<_CMP_UNORD_Q>(v, v);
        let safe = _mm512_mask_blend_pd(nan_mask, v, neg_inf);
        acc = _mm512_max_pd(acc, safe);
        saw_real |= !nan_mask;
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let load_mask: __mmask8 = (1u8 << tail) - 1;
        let v = _mm512_maskz_loadu_pd(load_mask, ptr.add(chunks * 8));
        let nan_mask = _mm512_cmp_pd_mask::<_CMP_UNORD_Q>(v, v);
        let real_mask = load_mask & !nan_mask;
        let safe = _mm512_mask_blend_pd(!real_mask, v, neg_inf);
        acc = _mm512_max_pd(acc, safe);
        saw_real |= real_mask;
    }

    if saw_real == 0 {
        return None;
    }
    Some(_mm512_reduce_max_pd(acc))
}

/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn min_f32_avx2(values: &[f32]) -> Option<f32> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    let ptr = values.as_ptr();
    let pos_inf = _mm256_set1_ps(f32::INFINITY);
    let mut acc = pos_inf;
    let mut saw_real = false;

    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm256_loadu_ps(ptr.add(i * 8));
        let nan_mask = _mm256_cmp_ps::<_CMP_UNORD_Q>(v, v);
        let safe = _mm256_blendv_ps(v, pos_inf, nan_mask);
        acc = _mm256_min_ps(acc, safe);
        let real_mask = _mm256_andnot_ps(nan_mask, _mm256_cmp_ps::<_CMP_EQ_OQ>(v, v));
        if _mm256_movemask_ps(real_mask) != 0 {
            saw_real = true;
        }
    }

    // Horizontal min of 8 × f32.
    let low = _mm256_castps256_ps128(acc);
    let high = _mm256_extractf128_ps::<1>(acc);
    let min128 = _mm_min_ps(low, high);
    let shuffled1 = _mm_movehl_ps(min128, min128);
    let min64 = _mm_min_ps(min128, shuffled1);
    let shuffled2 = _mm_shuffle_ps::<0b01>(min64, min64);
    let min32 = _mm_min_ss(min64, shuffled2);
    let mut result = _mm_cvtss_f32(min32);

    for i in (chunks * 8)..len {
        let v = values[i];
        if !v.is_nan() {
            if !saw_real || v < result {
                result = v;
            }
            saw_real = true;
        }
    }

    if saw_real { Some(result) } else { None }
}

/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn max_f32_avx2(values: &[f32]) -> Option<f32> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    let ptr = values.as_ptr();
    let neg_inf = _mm256_set1_ps(f32::NEG_INFINITY);
    let mut acc = neg_inf;
    let mut saw_real = false;

    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm256_loadu_ps(ptr.add(i * 8));
        let nan_mask = _mm256_cmp_ps::<_CMP_UNORD_Q>(v, v);
        let safe = _mm256_blendv_ps(v, neg_inf, nan_mask);
        acc = _mm256_max_ps(acc, safe);
        let real_mask = _mm256_andnot_ps(nan_mask, _mm256_cmp_ps::<_CMP_EQ_OQ>(v, v));
        if _mm256_movemask_ps(real_mask) != 0 {
            saw_real = true;
        }
    }

    let low = _mm256_castps256_ps128(acc);
    let high = _mm256_extractf128_ps::<1>(acc);
    let max128 = _mm_max_ps(low, high);
    let shuffled1 = _mm_movehl_ps(max128, max128);
    let max64 = _mm_max_ps(max128, shuffled1);
    let shuffled2 = _mm_shuffle_ps::<0b01>(max64, max64);
    let max32 = _mm_max_ss(max64, shuffled2);
    let mut result = _mm_cvtss_f32(max32);

    for i in (chunks * 8)..len {
        let v = values[i];
        if !v.is_nan() {
            if !saw_real || v > result {
                result = v;
            }
            saw_real = true;
        }
    }

    if saw_real { Some(result) } else { None }
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn min_f32_avx512(values: &[f32]) -> Option<f32> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    let ptr = values.as_ptr();
    let pos_inf = _mm512_set1_ps(f32::INFINITY);
    let mut acc = pos_inf;
    let mut saw_real: __mmask16 = 0;

    let chunks = len / 16;
    for i in 0..chunks {
        let v = _mm512_loadu_ps(ptr.add(i * 16));
        let nan_mask = _mm512_cmp_ps_mask::<_CMP_UNORD_Q>(v, v);
        let safe = _mm512_mask_blend_ps(nan_mask, v, pos_inf);
        acc = _mm512_min_ps(acc, safe);
        saw_real |= !nan_mask;
    }
    let tail = len - chunks * 16;
    if tail > 0 {
        let load_mask: __mmask16 = (1u16 << tail) - 1;
        let v = _mm512_maskz_loadu_ps(load_mask, ptr.add(chunks * 16));
        let nan_mask = _mm512_cmp_ps_mask::<_CMP_UNORD_Q>(v, v);
        let real_mask = load_mask & !nan_mask;
        let safe = _mm512_mask_blend_ps(!real_mask, v, pos_inf);
        acc = _mm512_min_ps(acc, safe);
        saw_real |= real_mask;
    }

    if saw_real == 0 {
        return None;
    }
    Some(_mm512_reduce_min_ps(acc))
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn max_f32_avx512(values: &[f32]) -> Option<f32> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    let ptr = values.as_ptr();
    let neg_inf = _mm512_set1_ps(f32::NEG_INFINITY);
    let mut acc = neg_inf;
    let mut saw_real: __mmask16 = 0;

    let chunks = len / 16;
    for i in 0..chunks {
        let v = _mm512_loadu_ps(ptr.add(i * 16));
        let nan_mask = _mm512_cmp_ps_mask::<_CMP_UNORD_Q>(v, v);
        let safe = _mm512_mask_blend_ps(nan_mask, v, neg_inf);
        acc = _mm512_max_ps(acc, safe);
        saw_real |= !nan_mask;
    }
    let tail = len - chunks * 16;
    if tail > 0 {
        let load_mask: __mmask16 = (1u16 << tail) - 1;
        let v = _mm512_maskz_loadu_ps(load_mask, ptr.add(chunks * 16));
        let nan_mask = _mm512_cmp_ps_mask::<_CMP_UNORD_Q>(v, v);
        let real_mask = load_mask & !nan_mask;
        let safe = _mm512_mask_blend_ps(!real_mask, v, neg_inf);
        acc = _mm512_max_ps(acc, safe);
        saw_real |= real_mask;
    }

    if saw_real == 0 {
        return None;
    }
    Some(_mm512_reduce_max_ps(acc))
}

// ── i64 / f64 compare kernels — packed Vec<u64> output ────────────────────────
//
// Each kernel produces one bit per element in a `Vec<u64>` (LSB-first
// per word). Four i64 lanes fit in one AVX2 step → 4 bits deposited
// at position `i * 4` of the output; eight lanes via AVX-512 → 8 bits.
// Since 64 is divisible by 4 and by 8, no lane deposit crosses a word
// boundary.

#[inline(always)]
fn i64_bitmap_len(len: usize) -> usize {
    len.div_ceil(64)
}

/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn eq_i64_avx2(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm256_set1_epi64x(scalar);
    let ptr = values.as_ptr();
    let chunks = len / 4;
    for i in 0..chunks {
        let v = _mm256_loadu_si256(ptr.add(i * 4) as *const __m256i);
        let cmp = _mm256_cmpeq_epi64(v, bcast);
        let mask = _mm256_movemask_pd(_mm256_castsi256_pd(cmp)) as u64 & 0xF;
        let bit_pos = i * 4;
        out[bit_pos / 64] |= mask << (bit_pos % 64);
    }
    for i in (chunks * 4)..len {
        if values[i] == scalar {
            out[i / 64] |= 1u64 << (i % 64);
        }
    }
    out
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn eq_i64_avx512(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm512_set1_epi64(scalar);
    let ptr = values.as_ptr() as *const i64;
    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm512_loadu_epi64(ptr.add(i * 8));
        let m: __mmask8 = _mm512_cmpeq_epi64_mask(v, bcast);
        let bit_pos = i * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let load_mask: __mmask8 = (1u8 << tail) - 1;
        let v = _mm512_maskz_loadu_epi64(load_mask, ptr.add(chunks * 8));
        let m: __mmask8 = _mm512_mask_cmpeq_epi64_mask(load_mask, v, bcast);
        let bit_pos = chunks * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    out
}

/// Inequality = NOT eq over the loaded lanes.
///
/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn ne_i64_avx2(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm256_set1_epi64x(scalar);
    let ptr = values.as_ptr();
    let chunks = len / 4;
    for i in 0..chunks {
        let v = _mm256_loadu_si256(ptr.add(i * 4) as *const __m256i);
        let cmp = _mm256_cmpeq_epi64(v, bcast);
        let mask = (!_mm256_movemask_pd(_mm256_castsi256_pd(cmp)) as u64) & 0xF;
        let bit_pos = i * 4;
        out[bit_pos / 64] |= mask << (bit_pos % 64);
    }
    for i in (chunks * 4)..len {
        if values[i] != scalar {
            out[i / 64] |= 1u64 << (i % 64);
        }
    }
    out
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn ne_i64_avx512(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm512_set1_epi64(scalar);
    let ptr = values.as_ptr() as *const i64;
    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm512_loadu_epi64(ptr.add(i * 8));
        let m: __mmask8 = !_mm512_cmpeq_epi64_mask(v, bcast);
        let bit_pos = i * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let load_mask: __mmask8 = (1u8 << tail) - 1;
        let v = _mm512_maskz_loadu_epi64(load_mask, ptr.add(chunks * 8));
        // `!eq` inside the loaded lanes only.
        let eq: __mmask8 = _mm512_mask_cmpeq_epi64_mask(load_mask, v, bcast);
        let m: __mmask8 = load_mask & !eq;
        let bit_pos = chunks * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    out
}

/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn gt_i64_avx2(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm256_set1_epi64x(scalar);
    let ptr = values.as_ptr();
    let chunks = len / 4;
    for i in 0..chunks {
        let v = _mm256_loadu_si256(ptr.add(i * 4) as *const __m256i);
        let cmp = _mm256_cmpgt_epi64(v, bcast);
        let mask = _mm256_movemask_pd(_mm256_castsi256_pd(cmp)) as u64 & 0xF;
        let bit_pos = i * 4;
        out[bit_pos / 64] |= mask << (bit_pos % 64);
    }
    for i in (chunks * 4)..len {
        if values[i] > scalar {
            out[i / 64] |= 1u64 << (i % 64);
        }
    }
    out
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn gt_i64_avx512(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm512_set1_epi64(scalar);
    let ptr = values.as_ptr() as *const i64;
    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm512_loadu_epi64(ptr.add(i * 8));
        let m: __mmask8 = _mm512_cmpgt_epi64_mask(v, bcast);
        let bit_pos = i * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let load_mask: __mmask8 = (1u8 << tail) - 1;
        let v = _mm512_maskz_loadu_epi64(load_mask, ptr.add(chunks * 8));
        let m: __mmask8 = _mm512_mask_cmpgt_epi64_mask(load_mask, v, bcast);
        let bit_pos = chunks * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    out
}

/// lt(a, s) = gt(s, a): derive via operand swap.
///
/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn lt_i64_avx2(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm256_set1_epi64x(scalar);
    let ptr = values.as_ptr();
    let chunks = len / 4;
    for i in 0..chunks {
        let v = _mm256_loadu_si256(ptr.add(i * 4) as *const __m256i);
        // scalar > v  ⇔  v < scalar
        let cmp = _mm256_cmpgt_epi64(bcast, v);
        let mask = _mm256_movemask_pd(_mm256_castsi256_pd(cmp)) as u64 & 0xF;
        let bit_pos = i * 4;
        out[bit_pos / 64] |= mask << (bit_pos % 64);
    }
    for i in (chunks * 4)..len {
        if values[i] < scalar {
            out[i / 64] |= 1u64 << (i % 64);
        }
    }
    out
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn lt_i64_avx512(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm512_set1_epi64(scalar);
    let ptr = values.as_ptr() as *const i64;
    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm512_loadu_epi64(ptr.add(i * 8));
        let m: __mmask8 = _mm512_cmplt_epi64_mask(v, bcast);
        let bit_pos = i * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let load_mask: __mmask8 = (1u8 << tail) - 1;
        let v = _mm512_maskz_loadu_epi64(load_mask, ptr.add(chunks * 8));
        let m: __mmask8 = _mm512_mask_cmplt_epi64_mask(load_mask, v, bcast);
        let bit_pos = chunks * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    out
}

/// ge(a, s) = NOT lt(a, s).
///
/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn ge_i64_avx2(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm256_set1_epi64x(scalar);
    let ptr = values.as_ptr();
    let chunks = len / 4;
    for i in 0..chunks {
        let v = _mm256_loadu_si256(ptr.add(i * 4) as *const __m256i);
        // lt = cmpgt(scalar, v); ge = NOT lt.
        let cmp = _mm256_cmpgt_epi64(bcast, v);
        let mask = (!_mm256_movemask_pd(_mm256_castsi256_pd(cmp)) as u64) & 0xF;
        let bit_pos = i * 4;
        out[bit_pos / 64] |= mask << (bit_pos % 64);
    }
    for i in (chunks * 4)..len {
        if values[i] >= scalar {
            out[i / 64] |= 1u64 << (i % 64);
        }
    }
    out
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn ge_i64_avx512(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm512_set1_epi64(scalar);
    let ptr = values.as_ptr() as *const i64;
    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm512_loadu_epi64(ptr.add(i * 8));
        let m: __mmask8 = _mm512_cmpge_epi64_mask(v, bcast);
        let bit_pos = i * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let load_mask: __mmask8 = (1u8 << tail) - 1;
        let v = _mm512_maskz_loadu_epi64(load_mask, ptr.add(chunks * 8));
        let m: __mmask8 = _mm512_mask_cmpge_epi64_mask(load_mask, v, bcast);
        let bit_pos = chunks * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    out
}

/// le(a, s) = NOT gt(a, s).
///
/// # Safety
/// Host must advertise `avx2`.
#[target_feature(enable = "avx2")]
pub unsafe fn le_i64_avx2(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm256_set1_epi64x(scalar);
    let ptr = values.as_ptr();
    let chunks = len / 4;
    for i in 0..chunks {
        let v = _mm256_loadu_si256(ptr.add(i * 4) as *const __m256i);
        let cmp = _mm256_cmpgt_epi64(v, bcast);
        let mask = (!_mm256_movemask_pd(_mm256_castsi256_pd(cmp)) as u64) & 0xF;
        let bit_pos = i * 4;
        out[bit_pos / 64] |= mask << (bit_pos % 64);
    }
    for i in (chunks * 4)..len {
        if values[i] <= scalar {
            out[i / 64] |= 1u64 << (i % 64);
        }
    }
    out
}

/// # Safety
/// Host must advertise `avx512f`.
#[target_feature(enable = "avx512f")]
pub unsafe fn le_i64_avx512(values: &[i64], scalar: i64) -> Vec<u64> {
    let len = values.len();
    let mut out = vec![0u64; i64_bitmap_len(len)];
    let bcast = _mm512_set1_epi64(scalar);
    let ptr = values.as_ptr() as *const i64;
    let chunks = len / 8;
    for i in 0..chunks {
        let v = _mm512_loadu_epi64(ptr.add(i * 8));
        let m: __mmask8 = _mm512_cmple_epi64_mask(v, bcast);
        let bit_pos = i * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    let tail = len - chunks * 8;
    if tail > 0 {
        let load_mask: __mmask8 = (1u8 << tail) - 1;
        let v = _mm512_maskz_loadu_epi64(load_mask, ptr.add(chunks * 8));
        let m: __mmask8 = _mm512_mask_cmple_epi64_mask(load_mask, v, bcast);
        let bit_pos = chunks * 8;
        out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
    }
    out
}

// ── f64 compare kernels ───────────────────────────────────────────────────────
//
// IEEE ordered semantics: `eq/lt/le/gt/ge` return false if either
// operand is NaN. `ne` uses the _NEQ_UQ predicate (unordered not-equal)
// so that NaN vs anything is `true`, matching Rust's `!=` on f64.

macro_rules! f64_cmp_avx2 {
    ($name:ident, $imm:ident) => {
        /// # Safety
        /// Host must advertise `avx2`.
        #[target_feature(enable = "avx2")]
        pub unsafe fn $name(values: &[f64], scalar: f64) -> Vec<u64> {
            let len = values.len();
            let mut out = vec![0u64; i64_bitmap_len(len)];
            let bcast = _mm256_set1_pd(scalar);
            let ptr = values.as_ptr();
            let chunks = len / 4;
            for i in 0..chunks {
                let v = _mm256_loadu_pd(ptr.add(i * 4));
                let cmp = _mm256_cmp_pd::<{ $imm }>(v, bcast);
                let mask = _mm256_movemask_pd(cmp) as u64 & 0xF;
                let bit_pos = i * 4;
                out[bit_pos / 64] |= mask << (bit_pos % 64);
            }
            for i in (chunks * 4)..len {
                let v = values[i];
                let keep = match $imm {
                    _CMP_EQ_OQ => v == scalar,
                    _CMP_NEQ_UQ => v != scalar,
                    _CMP_LT_OQ => v < scalar,
                    _CMP_LE_OQ => v <= scalar,
                    _CMP_GT_OQ => v > scalar,
                    _CMP_GE_OQ => v >= scalar,
                    _ => unreachable!(),
                };
                if keep {
                    out[i / 64] |= 1u64 << (i % 64);
                }
            }
            out
        }
    };
}
f64_cmp_avx2!(eq_f64_avx2, _CMP_EQ_OQ);
f64_cmp_avx2!(ne_f64_avx2, _CMP_NEQ_UQ);
f64_cmp_avx2!(lt_f64_avx2, _CMP_LT_OQ);
f64_cmp_avx2!(le_f64_avx2, _CMP_LE_OQ);
f64_cmp_avx2!(gt_f64_avx2, _CMP_GT_OQ);
f64_cmp_avx2!(ge_f64_avx2, _CMP_GE_OQ);

macro_rules! f64_cmp_avx512 {
    ($name:ident, $imm:ident) => {
        /// # Safety
        /// Host must advertise `avx512f`.
        #[target_feature(enable = "avx512f")]
        pub unsafe fn $name(values: &[f64], scalar: f64) -> Vec<u64> {
            let len = values.len();
            let mut out = vec![0u64; i64_bitmap_len(len)];
            let bcast = _mm512_set1_pd(scalar);
            let ptr = values.as_ptr();
            let chunks = len / 8;
            for i in 0..chunks {
                let v = _mm512_loadu_pd(ptr.add(i * 8));
                let m: __mmask8 = _mm512_cmp_pd_mask::<{ $imm }>(v, bcast);
                let bit_pos = i * 8;
                out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
            }
            let tail = len - chunks * 8;
            if tail > 0 {
                let load_mask: __mmask8 = (1u8 << tail) - 1;
                let v = _mm512_maskz_loadu_pd(load_mask, ptr.add(chunks * 8));
                let m: __mmask8 = _mm512_mask_cmp_pd_mask::<{ $imm }>(load_mask, v, bcast);
                let bit_pos = chunks * 8;
                out[bit_pos / 64] |= (m as u64) << (bit_pos % 64);
            }
            out
        }
    };
}
f64_cmp_avx512!(eq_f64_avx512, _CMP_EQ_OQ);
f64_cmp_avx512!(ne_f64_avx512, _CMP_NEQ_UQ);
f64_cmp_avx512!(lt_f64_avx512, _CMP_LT_OQ);
f64_cmp_avx512!(le_f64_avx512, _CMP_LE_OQ);
f64_cmp_avx512!(gt_f64_avx512, _CMP_GT_OQ);
f64_cmp_avx512!(ge_f64_avx512, _CMP_GE_OQ);
