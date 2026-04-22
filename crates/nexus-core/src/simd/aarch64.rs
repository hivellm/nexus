//! AArch64 NEON SIMD kernels.
//!
//! NEON is the ARMv8 baseline — `std::arch::is_aarch64_feature_detected!("neon")`
//! returns true on every ARMv8 CPU. The `#[target_feature(enable = "neon")]`
//! attribute is therefore a belt-and-braces guarantee, not a
//! conditional.
//!
//! The module mirrors `simd::x86` one-to-one: the dispatch layer
//! treats both as interchangeable function pointers behind the same
//! `OnceLock<unsafe fn>`.

#![cfg(target_arch = "aarch64")]

use std::arch::aarch64::*;

// ── f32 dot product ───────────────────────────────────────────────────────────

/// Dot product, NEON path (4 lanes of f32).
///
/// # Safety
/// Host must advertise `neon` (always true on ARMv8).
#[target_feature(enable = "neon")]
pub unsafe fn dot_f32_neon(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let chunks = len / 4;
    let mut acc = vdupq_n_f32(0.0);
    let ap = a.as_ptr();
    let bp = b.as_ptr();

    for i in 0..chunks {
        let av = vld1q_f32(ap.add(i * 4));
        let bv = vld1q_f32(bp.add(i * 4));
        acc = vfmaq_f32(acc, av, bv);
    }

    let mut result = vaddvq_f32(acc);
    for i in (chunks * 4)..len {
        result += *ap.add(i) * *bp.add(i);
    }
    result
}

/// # Safety
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn l2_sq_f32_neon(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let chunks = len / 4;
    let mut acc = vdupq_n_f32(0.0);
    let ap = a.as_ptr();
    let bp = b.as_ptr();

    for i in 0..chunks {
        let av = vld1q_f32(ap.add(i * 4));
        let bv = vld1q_f32(bp.add(i * 4));
        let d = vsubq_f32(av, bv);
        acc = vfmaq_f32(acc, d, d);
    }

    let mut result = vaddvq_f32(acc);
    for i in (chunks * 4)..len {
        let d = *ap.add(i) - *bp.add(i);
        result += d * d;
    }
    result
}

/// # Safety
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn cosine_f32_neon(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_f32_neon(a, b);
    let na = dot_f32_neon(a, a);
    let nb = dot_f32_neon(b, b);
    let denom = (na * nb).sqrt();
    if denom == 0.0 { 0.0 } else { 1.0 - dot / denom }
}

/// # Safety
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn normalize_f32_neon(v: &mut [f32]) -> f32 {
    let sq = dot_f32_neon(v, v);
    let norm = sq.sqrt();
    if norm > 0.0 {
        let inv = 1.0 / norm;
        let inv_v = vdupq_n_f32(inv);
        let len = v.len();
        let chunks = len / 4;
        let vp = v.as_mut_ptr();
        for i in 0..chunks {
            let p = vp.add(i * 4);
            vst1q_f32(p, vmulq_f32(vld1q_f32(p), inv_v));
        }
        for i in (chunks * 4)..len {
            *vp.add(i) *= inv;
        }
    }
    norm
}

// ── reductions ───────────────────────────────────────────────────────────────

/// # Safety
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn sum_i64_neon(values: &[i64]) -> i64 {
    let len = values.len();
    let ptr = values.as_ptr();
    let mut acc = vdupq_n_s64(0);
    let chunks = len / 2;
    for i in 0..chunks {
        acc = vaddq_s64(acc, vld1q_s64(ptr.add(i * 2)));
    }
    let mut result = vgetq_lane_s64::<0>(acc).wrapping_add(vgetq_lane_s64::<1>(acc));
    for i in (chunks * 2)..len {
        result = result.wrapping_add(values[i]);
    }
    result
}

/// # Safety
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn sum_f64_neon(values: &[f64]) -> f64 {
    let len = values.len();
    let ptr = values.as_ptr();
    let mut acc = vdupq_n_f64(0.0);
    let chunks = len / 2;
    for i in 0..chunks {
        acc = vaddq_f64(acc, vld1q_f64(ptr.add(i * 2)));
    }
    let mut result = vaddvq_f64(acc);
    for i in (chunks * 2)..len {
        result += values[i];
    }
    result
}

/// # Safety
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn sum_f32_neon(values: &[f32]) -> f32 {
    let len = values.len();
    let ptr = values.as_ptr();
    let mut acc = vdupq_n_f32(0.0);
    let chunks = len / 4;
    for i in 0..chunks {
        acc = vaddq_f32(acc, vld1q_f32(ptr.add(i * 4)));
    }
    let mut result = vaddvq_f32(acc);
    for i in (chunks * 4)..len {
        result += values[i];
    }
    result
}

/// # Safety
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn min_f64_neon(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let len = values.len();
    let ptr = values.as_ptr();
    let pos_inf = vdupq_n_f64(f64::INFINITY);
    let mut acc = pos_inf;
    let mut saw_real = false;
    let chunks = len / 2;
    for i in 0..chunks {
        let v = vld1q_f64(ptr.add(i * 2));
        // NaN-safe replacement: where v != v, substitute +inf.
        let nan_mask = vceqq_f64(v, v); // true where NOT NaN
        let v_safe = vbslq_f64(nan_mask, v, pos_inf);
        acc = vminq_f64(acc, v_safe);
        // vmaxvq_u32 over the mask tells us if any lane was a real value.
        let cast = vreinterpretq_u32_u64(nan_mask);
        if vmaxvq_u32(cast) != 0 {
            saw_real = true;
        }
    }
    let mut result = vminvq_f64(acc);
    for i in (chunks * 2)..len {
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
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn max_f64_neon(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let len = values.len();
    let ptr = values.as_ptr();
    let neg_inf = vdupq_n_f64(f64::NEG_INFINITY);
    let mut acc = neg_inf;
    let mut saw_real = false;
    let chunks = len / 2;
    for i in 0..chunks {
        let v = vld1q_f64(ptr.add(i * 2));
        let nan_mask = vceqq_f64(v, v);
        let v_safe = vbslq_f64(nan_mask, v, neg_inf);
        acc = vmaxq_f64(acc, v_safe);
        let cast = vreinterpretq_u32_u64(nan_mask);
        if vmaxvq_u32(cast) != 0 {
            saw_real = true;
        }
    }
    let mut result = vmaxvq_f64(acc);
    for i in (chunks * 2)..len {
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
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn min_f32_neon(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let len = values.len();
    let ptr = values.as_ptr();
    let pos_inf = vdupq_n_f32(f32::INFINITY);
    let mut acc = pos_inf;
    let mut saw_real = false;
    let chunks = len / 4;
    for i in 0..chunks {
        let v = vld1q_f32(ptr.add(i * 4));
        let nan_mask = vceqq_f32(v, v); // true where NOT NaN
        let v_safe = vbslq_f32(nan_mask, v, pos_inf);
        acc = vminq_f32(acc, v_safe);
        if vmaxvq_u32(nan_mask) != 0 {
            saw_real = true;
        }
    }
    let mut result = vminvq_f32(acc);
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
/// Host must advertise `neon`.
#[target_feature(enable = "neon")]
pub unsafe fn max_f32_neon(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let len = values.len();
    let ptr = values.as_ptr();
    let neg_inf = vdupq_n_f32(f32::NEG_INFINITY);
    let mut acc = neg_inf;
    let mut saw_real = false;
    let chunks = len / 4;
    for i in 0..chunks {
        let v = vld1q_f32(ptr.add(i * 4));
        let nan_mask = vceqq_f32(v, v);
        let v_safe = vbslq_f32(nan_mask, v, neg_inf);
        acc = vmaxq_f32(acc, v_safe);
        if vmaxvq_u32(nan_mask) != 0 {
            saw_real = true;
        }
    }
    let mut result = vmaxvq_f32(acc);
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
