//! Safe dispatch layer for f32 distance kernels.
//!
//! Each public function exposes the same signature as its scalar
//! reference in [`crate::simd::scalar`] but internally routes to the
//! fastest kernel the runtime CPU supports. The selection happens
//! exactly once per operation per process: the first call probes
//! [`super::dispatch::cpu()`] and caches the resulting function
//! pointer inside a [`OnceLock`], so the hot path is a single
//! indirect call with no feature-detection overhead.
//!
//! Safety: the wrappers assert `a.len() == b.len()` in debug builds,
//! and only call `unsafe` kernels whose `target_feature` has been
//! proven by the CPU probe.

use std::sync::OnceLock;

use super::dispatch::cpu;
use super::scalar;

// ── dot_f32 ───────────────────────────────────────────────────────────────────

type DotF32Fn = unsafe fn(&[f32], &[f32]) -> f32;
static DOT_F32: OnceLock<DotF32Fn> = OnceLock::new();

fn pick_dot_f32() -> DotF32Fn {
    let features = cpu();
    if features.disabled {
        return wrap_scalar_dot_f32;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::dot_f32_avx512;
        }
        if features.avx2 {
            return super::x86::dot_f32_avx2;
        }
        if features.sse42 {
            return super::x86::dot_f32_sse42;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::dot_f32_neon;
        }
    }
    wrap_scalar_dot_f32
}

/// Adapter so the scalar (safe) kernel fits the `unsafe fn` contract.
///
/// # Safety
/// Always sound to call; the `unsafe` is only for type unification
/// with the SIMD kernels.
unsafe fn wrap_scalar_dot_f32(a: &[f32], b: &[f32]) -> f32 {
    scalar::dot_f32(a, b)
}

/// Safe dispatch entry for the dot product.
#[inline]
pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "simd::distance::dot_f32: length mismatch");
    let kernel = DOT_F32.get_or_init(pick_dot_f32);
    // SAFETY: `pick_dot_f32` only returns kernels whose target_feature
    // is verified present on the host via `cpu()`.
    unsafe { kernel(a, b) }
}

// ── l2_sq_f32 ─────────────────────────────────────────────────────────────────

type L2SqF32Fn = unsafe fn(&[f32], &[f32]) -> f32;
static L2_SQ_F32: OnceLock<L2SqF32Fn> = OnceLock::new();

fn pick_l2_sq_f32() -> L2SqF32Fn {
    let features = cpu();
    if features.disabled {
        return wrap_scalar_l2_sq_f32;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::l2_sq_f32_avx512;
        }
        if features.avx2 {
            return super::x86::l2_sq_f32_avx2;
        }
        if features.sse42 {
            return super::x86::l2_sq_f32_sse42;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::l2_sq_f32_neon;
        }
    }
    wrap_scalar_l2_sq_f32
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_scalar_l2_sq_f32(a: &[f32], b: &[f32]) -> f32 {
    scalar::l2_sq_f32(a, b)
}

/// Safe dispatch entry for squared L2 distance.
#[inline]
pub fn l2_sq_f32(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(
        a.len(),
        b.len(),
        "simd::distance::l2_sq_f32: length mismatch"
    );
    let kernel = L2_SQ_F32.get_or_init(pick_l2_sq_f32);
    // SAFETY: see `dot_f32` above.
    unsafe { kernel(a, b) }
}

// ── cosine_f32 ────────────────────────────────────────────────────────────────

type CosineF32Fn = unsafe fn(&[f32], &[f32]) -> f32;
static COSINE_F32: OnceLock<CosineF32Fn> = OnceLock::new();

fn pick_cosine_f32() -> CosineF32Fn {
    let features = cpu();
    if features.disabled {
        return wrap_scalar_cosine_f32;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::cosine_f32_avx512;
        }
        if features.avx2 {
            return super::x86::cosine_f32_avx2;
        }
        if features.sse42 {
            return super::x86::cosine_f32_sse42;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::cosine_f32_neon;
        }
    }
    wrap_scalar_cosine_f32
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_scalar_cosine_f32(a: &[f32], b: &[f32]) -> f32 {
    scalar::cosine_f32(a, b)
}

/// Safe dispatch entry for cosine distance.
#[inline]
pub fn cosine_f32(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(
        a.len(),
        b.len(),
        "simd::distance::cosine_f32: length mismatch"
    );
    let kernel = COSINE_F32.get_or_init(pick_cosine_f32);
    // SAFETY: see `dot_f32` above.
    unsafe { kernel(a, b) }
}

// ── normalize_f32 ─────────────────────────────────────────────────────────────

type NormalizeF32Fn = unsafe fn(&mut [f32]) -> f32;
static NORMALIZE_F32: OnceLock<NormalizeF32Fn> = OnceLock::new();

fn pick_normalize_f32() -> NormalizeF32Fn {
    let features = cpu();
    if features.disabled {
        return wrap_scalar_normalize_f32;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::normalize_f32_avx512;
        }
        if features.avx2 {
            return super::x86::normalize_f32_avx2;
        }
        if features.sse42 {
            return super::x86::normalize_f32_sse42;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::normalize_f32_neon;
        }
    }
    wrap_scalar_normalize_f32
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_scalar_normalize_f32(v: &mut [f32]) -> f32 {
    scalar::normalize_f32(v)
}

/// Safe dispatch entry for in-place L2 normalisation.
///
/// Returns the pre-normalisation norm. Zero-norm vectors are left
/// unchanged and the function returns `0.0`.
#[inline]
pub fn normalize_f32(v: &mut [f32]) -> f32 {
    let kernel = NORMALIZE_F32.get_or_init(pick_normalize_f32);
    // SAFETY: see `dot_f32` above.
    unsafe { kernel(v) }
}

// ── Observability helpers ────────────────────────────────────────────────────

/// Name of the kernel tier currently bound to each distance op.
///
/// Returns `("dot", "avx2")` / `("cosine", "scalar")` / ... tuples.
/// The list is stable and safe to expose via `/stats` or logs.
pub fn kernel_tiers() -> [(&'static str, &'static str); 4] {
    // Force initialisation so the tier we report is the one that will
    // actually be used on the hot path.
    let _ = DOT_F32.get_or_init(pick_dot_f32);
    let _ = L2_SQ_F32.get_or_init(pick_l2_sq_f32);
    let _ = COSINE_F32.get_or_init(pick_cosine_f32);
    let _ = NORMALIZE_F32.get_or_init(pick_normalize_f32);
    let tier = cpu().preferred_tier();
    [
        ("dot_f32", tier),
        ("l2_sq_f32", tier),
        ("cosine_f32", tier),
        ("normalize_f32", tier),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_matches_scalar_for_small_vectors() {
        let a: Vec<f32> = (0..37).map(|i| (i as f32) * 0.25 - 4.0).collect();
        let b: Vec<f32> = (0..37).map(|i| ((i as f32) * 0.1).sin()).collect();

        let s_dot = scalar::dot_f32(&a, &b);
        let d_dot = dot_f32(&a, &b);
        assert!(
            (s_dot - d_dot).abs() < 1e-4,
            "scalar {s_dot} vs dispatch {d_dot}"
        );

        let s_l2 = scalar::l2_sq_f32(&a, &b);
        let d_l2 = l2_sq_f32(&a, &b);
        assert!(
            (s_l2 - d_l2).abs() < 1e-4,
            "scalar {s_l2} vs dispatch {d_l2}"
        );

        let s_cos = scalar::cosine_f32(&a, &b);
        let d_cos = cosine_f32(&a, &b);
        assert!(
            (s_cos - d_cos).abs() < 1e-4,
            "scalar {s_cos} vs dispatch {d_cos}"
        );

        let mut s_norm = a.clone();
        let mut d_norm = a.clone();
        scalar::normalize_f32(&mut s_norm);
        normalize_f32(&mut d_norm);
        for (i, (sv, dv)) in s_norm.iter().zip(d_norm.iter()).enumerate() {
            assert!(
                (sv - dv).abs() < 1e-5,
                "normalize lane {i}: scalar {sv} vs dispatch {dv}"
            );
        }
    }

    #[test]
    fn dispatch_handles_various_sizes() {
        // Cover all the tail-handling paths: full chunk, partial, and
        // sub-chunk lengths for each lane width (4/8/16).
        for &len in &[
            1usize, 3, 4, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 768, 1024,
        ] {
            let a: Vec<f32> = (0..len).map(|i| (i as f32).sin()).collect();
            let b: Vec<f32> = (0..len).map(|i| (i as f32).cos()).collect();
            let scalar_result = scalar::dot_f32(&a, &b);
            let dispatch_result = dot_f32(&a, &b);
            let tol = 1e-4_f32 * (len as f32).max(1.0);
            assert!(
                (scalar_result - dispatch_result).abs() < tol,
                "len={len}: scalar={scalar_result} dispatch={dispatch_result}"
            );
        }
    }

    #[test]
    fn kernel_tiers_returns_all_four_ops() {
        let tiers = kernel_tiers();
        let names: Vec<_> = tiers.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            ["dot_f32", "l2_sq_f32", "cosine_f32", "normalize_f32"]
        );
        // All ops share the same preferred tier.
        let tier = tiers[0].1;
        assert!(tiers.iter().all(|(_, t)| *t == tier));
    }
}
