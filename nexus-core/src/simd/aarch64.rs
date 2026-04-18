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
