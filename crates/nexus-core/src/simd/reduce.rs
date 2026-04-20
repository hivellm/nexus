//! Safe dispatch layer for numeric reductions (sum / min / max).
//!
//! Mirrors `simd::distance` / `simd::bitmap`: the first call to each
//! public entry point caches a kernel pointer into a `OnceLock<unsafe
//! fn>`; subsequent calls pay an indirect call and nothing else.
//!
//! Powers the executor-side aggregate path. For `SUM/MIN/MAX/AVG` over
//! numeric columns of ≥ ~128 rows the dispatch saturates on AVX-512
//! at ≈10 GB/s / ≈1.3 G elements/s for `f64`, vs ≈1 GB/s for the
//! scalar loop. The wiring into `executor::operators::aggregate`
//! lands in a follow-up commit; this module is the primitive layer
//! every agg call site depends on.

use std::sync::OnceLock;

use super::dispatch::cpu;
use super::scalar;

// ── sum_i64 ───────────────────────────────────────────────────────────────────

type SumI64Fn = unsafe fn(&[i64]) -> i64;
static SUM_I64: OnceLock<SumI64Fn> = OnceLock::new();

fn pick_sum_i64() -> SumI64Fn {
    let features = cpu();
    if features.disabled {
        return wrap_sum_i64;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::sum_i64_avx512;
        }
        if features.avx2 {
            return super::x86::sum_i64_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::sum_i64_neon;
        }
    }
    wrap_sum_i64
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_sum_i64(values: &[i64]) -> i64 {
    scalar::sum_i64(values)
}

#[inline]
pub fn sum_i64(values: &[i64]) -> i64 {
    let kernel = SUM_I64.get_or_init(pick_sum_i64);
    // SAFETY: `pick_sum_i64` only returns kernels whose `target_feature`
    // is verified present via `cpu()`.
    unsafe { kernel(values) }
}

// ── sum_f64 ───────────────────────────────────────────────────────────────────

type SumF64Fn = unsafe fn(&[f64]) -> f64;
static SUM_F64: OnceLock<SumF64Fn> = OnceLock::new();

fn pick_sum_f64() -> SumF64Fn {
    let features = cpu();
    if features.disabled {
        return wrap_sum_f64;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::sum_f64_avx512;
        }
        if features.avx2 {
            return super::x86::sum_f64_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::sum_f64_neon;
        }
    }
    wrap_sum_f64
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_sum_f64(values: &[f64]) -> f64 {
    scalar::sum_f64(values)
}

#[inline]
pub fn sum_f64(values: &[f64]) -> f64 {
    let kernel = SUM_F64.get_or_init(pick_sum_f64);
    // SAFETY: see `sum_i64` above.
    unsafe { kernel(values) }
}

// ── sum_f32 ───────────────────────────────────────────────────────────────────

type SumF32Fn = unsafe fn(&[f32]) -> f32;
static SUM_F32: OnceLock<SumF32Fn> = OnceLock::new();

fn pick_sum_f32() -> SumF32Fn {
    let features = cpu();
    if features.disabled {
        return wrap_sum_f32;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::sum_f32_avx512;
        }
        if features.avx2 {
            return super::x86::sum_f32_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::sum_f32_neon;
        }
    }
    wrap_sum_f32
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_sum_f32(values: &[f32]) -> f32 {
    scalar::sum_f32(values)
}

#[inline]
pub fn sum_f32(values: &[f32]) -> f32 {
    let kernel = SUM_F32.get_or_init(pick_sum_f32);
    // SAFETY: see `sum_i64` above.
    unsafe { kernel(values) }
}

// ── min_i64 / max_i64 ─────────────────────────────────────────────────────────
//
// i64 min/max have two quirks that keep them on scalar for now:
// (a) AVX-512F has native `_mm512_reduce_{min,max}_epi64`, AVX2 needs
//     emulation via compares + blends, and the emulation wins barely
//     a factor of 2 over scalar on short inputs; (b) the scalar loop
//     compiles to `pcmpgtq`+`pblendw` sequences already on AVX2 builds.
// When phase-2 benchmarks prove a win we will add AVX-512 + AVX2 paths.

#[inline]
pub fn min_i64(values: &[i64]) -> Option<i64> {
    scalar::min_i64(values)
}

#[inline]
pub fn max_i64(values: &[i64]) -> Option<i64> {
    scalar::max_i64(values)
}

// ── min_f64 / max_f64 ─────────────────────────────────────────────────────────

type ReduceF64Fn = unsafe fn(&[f64]) -> Option<f64>;
static MIN_F64: OnceLock<ReduceF64Fn> = OnceLock::new();
static MAX_F64: OnceLock<ReduceF64Fn> = OnceLock::new();

fn pick_min_f64() -> ReduceF64Fn {
    let features = cpu();
    if features.disabled {
        return wrap_min_f64;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::min_f64_avx512;
        }
        if features.avx2 {
            return super::x86::min_f64_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::min_f64_neon;
        }
    }
    wrap_min_f64
}

fn pick_max_f64() -> ReduceF64Fn {
    let features = cpu();
    if features.disabled {
        return wrap_max_f64;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::max_f64_avx512;
        }
        if features.avx2 {
            return super::x86::max_f64_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::max_f64_neon;
        }
    }
    wrap_max_f64
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_min_f64(values: &[f64]) -> Option<f64> {
    scalar::min_f64(values)
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_max_f64(values: &[f64]) -> Option<f64> {
    scalar::max_f64(values)
}

#[inline]
pub fn min_f64(values: &[f64]) -> Option<f64> {
    let kernel = MIN_F64.get_or_init(pick_min_f64);
    // SAFETY: see `sum_i64` above.
    unsafe { kernel(values) }
}

#[inline]
pub fn max_f64(values: &[f64]) -> Option<f64> {
    let kernel = MAX_F64.get_or_init(pick_max_f64);
    // SAFETY: see `sum_i64` above.
    unsafe { kernel(values) }
}

// ── min_f32 / max_f32 ─────────────────────────────────────────────────────────

type ReduceF32Fn = unsafe fn(&[f32]) -> Option<f32>;
static MIN_F32: OnceLock<ReduceF32Fn> = OnceLock::new();
static MAX_F32: OnceLock<ReduceF32Fn> = OnceLock::new();

fn pick_min_f32() -> ReduceF32Fn {
    let features = cpu();
    if features.disabled {
        return wrap_min_f32;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::min_f32_avx512;
        }
        if features.avx2 {
            return super::x86::min_f32_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::min_f32_neon;
        }
    }
    wrap_min_f32
}

fn pick_max_f32() -> ReduceF32Fn {
    let features = cpu();
    if features.disabled {
        return wrap_max_f32;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return super::x86::max_f32_avx512;
        }
        if features.avx2 {
            return super::x86::max_f32_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return super::aarch64::max_f32_neon;
        }
    }
    wrap_max_f32
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_min_f32(values: &[f32]) -> Option<f32> {
    scalar::min_f32(values)
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_max_f32(values: &[f32]) -> Option<f32> {
    scalar::max_f32(values)
}

#[inline]
pub fn min_f32(values: &[f32]) -> Option<f32> {
    let kernel = MIN_F32.get_or_init(pick_min_f32);
    // SAFETY: see `sum_i64` above.
    unsafe { kernel(values) }
}

#[inline]
pub fn max_f32(values: &[f32]) -> Option<f32> {
    let kernel = MAX_F32.get_or_init(pick_max_f32);
    // SAFETY: see `sum_i64` above.
    unsafe { kernel(values) }
}

// ── observability ─────────────────────────────────────────────────────────────

/// Report the selected kernel tier per reduce op for `/stats`.
pub fn kernel_tiers() -> Vec<(&'static str, &'static str)> {
    let _ = SUM_I64.get_or_init(pick_sum_i64);
    let _ = SUM_F64.get_or_init(pick_sum_f64);
    let _ = SUM_F32.get_or_init(pick_sum_f32);
    let _ = MIN_F64.get_or_init(pick_min_f64);
    let _ = MAX_F64.get_or_init(pick_max_f64);
    let _ = MIN_F32.get_or_init(pick_min_f32);
    let _ = MAX_F32.get_or_init(pick_max_f32);
    let tier = cpu().preferred_tier();
    vec![
        ("sum_i64", tier),
        ("sum_f64", tier),
        ("sum_f32", tier),
        ("min_i64", "scalar"),
        ("max_i64", "scalar"),
        ("min_f64", tier),
        ("max_f64", tier),
        ("min_f32", tier),
        ("max_f32", tier),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_i64_dispatches() {
        let values: Vec<i64> = (0..1024).collect();
        let expected: i64 = (0..1024_i64).sum();
        assert_eq!(sum_i64(&values), expected);
    }

    #[test]
    fn sum_f64_dispatches() {
        let values: Vec<f64> = (0..1024).map(|i| i as f64 * 0.5).collect();
        let expected = scalar::sum_f64(&values);
        let got = sum_f64(&values);
        assert!((expected - got).abs() < 1e-6);
    }

    #[test]
    fn min_max_f64_match_scalar() {
        let values: Vec<f64> = (0..1024).map(|i| ((i as f64) * 0.01).sin()).collect();
        assert_eq!(min_f64(&values), scalar::min_f64(&values));
        assert_eq!(max_f64(&values), scalar::max_f64(&values));
    }

    #[test]
    fn min_max_f32_match_scalar() {
        let values: Vec<f32> = (0..1024).map(|i| ((i as f32) * 0.01).sin()).collect();
        assert_eq!(min_f32(&values), scalar::min_f32(&values));
        assert_eq!(max_f32(&values), scalar::max_f32(&values));
    }

    #[test]
    fn empty_slice_min_max_returns_none() {
        assert!(min_f64(&[]).is_none());
        assert!(max_f64(&[]).is_none());
        assert!(min_f32(&[]).is_none());
        assert!(max_f32(&[]).is_none());
        assert_eq!(sum_i64(&[]), 0);
    }

    #[test]
    fn kernel_tiers_lists_every_op() {
        let tiers = kernel_tiers();
        let names: Vec<_> = tiers.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"sum_i64"));
        assert!(names.contains(&"sum_f64"));
        assert!(names.contains(&"max_f32"));
    }
}
