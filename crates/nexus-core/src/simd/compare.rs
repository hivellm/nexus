//! SIMD compare kernels producing packed bitmaps.
//!
//! Takes a typed column + scalar and returns a `Vec<u64>` in which the
//! `i`-th bit (LSB first within each word) is `1` when the predicate
//! holds for `values[i]` and `0` otherwise. The layout matches
//! `simd::bitmap` so the same popcount / and_popcount kernels can
//! consume the output to drive selection-vector gather in phase-2
//! filter integration.
//!
//! Supported ops:
//!
//! - i64: `eq`, `ne`, `lt`, `le`, `gt`, `ge`
//! - f64: `eq`, `ne`, `lt`, `le`, `gt`, `ge` (IEEE ordered semantics:
//!   any NaN operand makes the predicate return `false` — matches
//!   Cypher's three-valued logic collapsed to "not matched")
//!
//! f32 compare will land alongside phase-2 filter integration.

use std::sync::OnceLock;

use super::dispatch::cpu;
use super::scalar;

// ── dispatch entries ─────────────────────────────────────────────────────────

type CmpI64Fn = unsafe fn(&[i64], i64) -> Vec<u64>;
type CmpF64Fn = unsafe fn(&[f64], f64) -> Vec<u64>;

macro_rules! declare_i64_dispatch {
    ($name:ident, $pick:ident, $slot:ident, $scalar_fn:path, $avx2_fn:path, $avx512_fn:path) => {
        static $slot: OnceLock<CmpI64Fn> = OnceLock::new();

        fn $pick() -> CmpI64Fn {
            let features = cpu();
            if features.disabled {
                return $name;
            }
            #[cfg(target_arch = "x86_64")]
            {
                if features.avx512f {
                    return $avx512_fn;
                }
                if features.avx2 {
                    return $avx2_fn;
                }
            }
            $name
        }

        /// # Safety
        /// Adapter; always sound.
        unsafe fn $name(values: &[i64], scalar: i64) -> Vec<u64> {
            $scalar_fn(values, scalar)
        }
    };
}

macro_rules! declare_f64_dispatch {
    ($name:ident, $pick:ident, $slot:ident, $scalar_fn:path, $avx2_fn:path, $avx512_fn:path) => {
        static $slot: OnceLock<CmpF64Fn> = OnceLock::new();

        fn $pick() -> CmpF64Fn {
            let features = cpu();
            if features.disabled {
                return $name;
            }
            #[cfg(target_arch = "x86_64")]
            {
                if features.avx512f {
                    return $avx512_fn;
                }
                if features.avx2 {
                    return $avx2_fn;
                }
            }
            $name
        }

        /// # Safety
        /// Adapter; always sound.
        unsafe fn $name(values: &[f64], scalar: f64) -> Vec<u64> {
            $scalar_fn(values, scalar)
        }
    };
}

declare_i64_dispatch!(
    wrap_scalar_eq_i64,
    pick_eq_i64,
    EQ_I64,
    scalar::eq_i64,
    super::x86::eq_i64_avx2,
    super::x86::eq_i64_avx512
);
declare_i64_dispatch!(
    wrap_scalar_ne_i64,
    pick_ne_i64,
    NE_I64,
    scalar::ne_i64,
    super::x86::ne_i64_avx2,
    super::x86::ne_i64_avx512
);
declare_i64_dispatch!(
    wrap_scalar_lt_i64,
    pick_lt_i64,
    LT_I64,
    scalar::lt_i64,
    super::x86::lt_i64_avx2,
    super::x86::lt_i64_avx512
);
declare_i64_dispatch!(
    wrap_scalar_le_i64,
    pick_le_i64,
    LE_I64,
    scalar::le_i64,
    super::x86::le_i64_avx2,
    super::x86::le_i64_avx512
);
declare_i64_dispatch!(
    wrap_scalar_gt_i64,
    pick_gt_i64,
    GT_I64,
    scalar::gt_i64,
    super::x86::gt_i64_avx2,
    super::x86::gt_i64_avx512
);
declare_i64_dispatch!(
    wrap_scalar_ge_i64,
    pick_ge_i64,
    GE_I64,
    scalar::ge_i64,
    super::x86::ge_i64_avx2,
    super::x86::ge_i64_avx512
);

declare_f64_dispatch!(
    wrap_scalar_eq_f64,
    pick_eq_f64,
    EQ_F64,
    scalar::eq_f64,
    super::x86::eq_f64_avx2,
    super::x86::eq_f64_avx512
);
declare_f64_dispatch!(
    wrap_scalar_ne_f64,
    pick_ne_f64,
    NE_F64,
    scalar::ne_f64,
    super::x86::ne_f64_avx2,
    super::x86::ne_f64_avx512
);
declare_f64_dispatch!(
    wrap_scalar_lt_f64,
    pick_lt_f64,
    LT_F64,
    scalar::lt_f64,
    super::x86::lt_f64_avx2,
    super::x86::lt_f64_avx512
);
declare_f64_dispatch!(
    wrap_scalar_le_f64,
    pick_le_f64,
    LE_F64,
    scalar::le_f64,
    super::x86::le_f64_avx2,
    super::x86::le_f64_avx512
);
declare_f64_dispatch!(
    wrap_scalar_gt_f64,
    pick_gt_f64,
    GT_F64,
    scalar::gt_f64,
    super::x86::gt_f64_avx2,
    super::x86::gt_f64_avx512
);
declare_f64_dispatch!(
    wrap_scalar_ge_f64,
    pick_ge_f64,
    GE_F64,
    scalar::ge_f64,
    super::x86::ge_f64_avx2,
    super::x86::ge_f64_avx512
);

// ── public API (safe wrappers) ───────────────────────────────────────────────

macro_rules! public_i64_op {
    ($name:ident, $slot:ident, $pick:ident) => {
        #[inline]
        pub fn $name(values: &[i64], scalar: i64) -> Vec<u64> {
            let kernel = $slot.get_or_init($pick);
            // SAFETY: dispatch returns only kernels whose target_feature
            // is verified present at runtime via `cpu()`.
            unsafe { kernel(values, scalar) }
        }
    };
}
macro_rules! public_f64_op {
    ($name:ident, $slot:ident, $pick:ident) => {
        #[inline]
        pub fn $name(values: &[f64], scalar: f64) -> Vec<u64> {
            let kernel = $slot.get_or_init($pick);
            // SAFETY: see i64 macro.
            unsafe { kernel(values, scalar) }
        }
    };
}

public_i64_op!(eq_i64, EQ_I64, pick_eq_i64);
public_i64_op!(ne_i64, NE_I64, pick_ne_i64);
public_i64_op!(lt_i64, LT_I64, pick_lt_i64);
public_i64_op!(le_i64, LE_I64, pick_le_i64);
public_i64_op!(gt_i64, GT_I64, pick_gt_i64);
public_i64_op!(ge_i64, GE_I64, pick_ge_i64);
public_f64_op!(eq_f64, EQ_F64, pick_eq_f64);
public_f64_op!(ne_f64, NE_F64, pick_ne_f64);
public_f64_op!(lt_f64, LT_F64, pick_lt_f64);
public_f64_op!(le_f64, LE_F64, pick_le_f64);
public_f64_op!(gt_f64, GT_F64, pick_gt_f64);
public_f64_op!(ge_f64, GE_F64, pick_ge_f64);

// ── f32 compare ──────────────────────────────────────────────────────────────
//
// f32 dispatch currently bottoms out at the scalar kernel on every
// architecture. AVX2/AVX-512 f32 compare kernels will land alongside
// the SIMD-filter benchmark work in a follow-up slice of
// `phase3_executor-columnar-wiring`. The public entry points live
// here now so the executor-side columnar path has a single shape to
// target regardless of element width, and so CPU-dispatch upgrades
// become drop-in replacements.

#[inline]
pub fn eq_f32(values: &[f32], scalar: f32) -> Vec<u64> {
    scalar::eq_f32(values, scalar)
}
#[inline]
pub fn ne_f32(values: &[f32], scalar: f32) -> Vec<u64> {
    scalar::ne_f32(values, scalar)
}
#[inline]
pub fn lt_f32(values: &[f32], scalar: f32) -> Vec<u64> {
    scalar::lt_f32(values, scalar)
}
#[inline]
pub fn le_f32(values: &[f32], scalar: f32) -> Vec<u64> {
    scalar::le_f32(values, scalar)
}
#[inline]
pub fn gt_f32(values: &[f32], scalar: f32) -> Vec<u64> {
    scalar::gt_f32(values, scalar)
}
#[inline]
pub fn ge_f32(values: &[f32], scalar: f32) -> Vec<u64> {
    scalar::ge_f32(values, scalar)
}

// ── observability ────────────────────────────────────────────────────────────

/// Kernel tier per compare op for /stats.
pub fn kernel_tiers() -> Vec<(&'static str, &'static str)> {
    let _ = EQ_I64.get_or_init(pick_eq_i64);
    let _ = EQ_F64.get_or_init(pick_eq_f64);
    let tier = cpu().preferred_tier();
    vec![
        ("eq_i64", tier),
        ("ne_i64", tier),
        ("lt_i64", tier),
        ("le_i64", tier),
        ("gt_i64", tier),
        ("ge_i64", tier),
        ("eq_f64", tier),
        ("ne_f64", tier),
        ("lt_f64", tier),
        ("le_f64", tier),
        ("gt_f64", tier),
        ("ge_f64", tier),
        // f32 compare sits on the scalar kernel today; reported as such
        // so the /stats view matches reality.
        ("eq_f32", "scalar"),
        ("ne_f32", "scalar"),
        ("lt_f32", "scalar"),
        ("le_f32", "scalar"),
        ("gt_f32", "scalar"),
        ("ge_f32", "scalar"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_i64_dispatches() {
        let values = vec![1_i64, 5, 3, 5, 7, 5, 9, 5];
        let mask = eq_i64(&values, 5);
        let expected = scalar::eq_i64(&values, 5);
        assert_eq!(mask, expected);
    }

    #[test]
    fn lt_f64_dispatches() {
        let values: Vec<f64> = (0..37).map(|i| i as f64).collect();
        let mask = lt_f64(&values, 20.0);
        let expected = scalar::lt_f64(&values, 20.0);
        assert_eq!(mask, expected);
    }

    #[test]
    fn nan_makes_all_predicates_false() {
        let values = vec![f64::NAN, 1.0, f64::NAN, 2.0];
        // any predicate involving NaN → false
        let eq = eq_f64(&values, f64::NAN);
        assert_eq!(eq, vec![0u64]);
        let lt = lt_f64(&values, 10.0);
        assert_eq!(lt, scalar::lt_f64(&values, 10.0));
        // ne is `true` for NaN vs any (strict inequality), matches IEEE
        // semantics the scalar reference implements.
        let ne = ne_f64(&values, 1.0);
        assert_eq!(ne, scalar::ne_f64(&values, 1.0));
    }

    #[test]
    fn length_corner_cases_match_scalar() {
        for &n in &[0usize, 1, 3, 4, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128] {
            let values: Vec<i64> = (0..n as i64).collect();
            let got = lt_i64(&values, 50);
            let expected = scalar::lt_i64(&values, 50);
            assert_eq!(got, expected, "lt_i64 len={n}");
        }
    }

    #[test]
    fn f32_compare_parity() {
        // Dispatched (which today equals scalar) should match the scalar
        // reference for every op across a realistic range of values.
        let values: Vec<f32> = (0..257).map(|i| (i as f32) * 0.5).collect();
        let scalar_val = 32.5_f32;
        assert_eq!(
            eq_f32(&values, scalar_val),
            scalar::eq_f32(&values, scalar_val)
        );
        assert_eq!(
            ne_f32(&values, scalar_val),
            scalar::ne_f32(&values, scalar_val)
        );
        assert_eq!(
            lt_f32(&values, scalar_val),
            scalar::lt_f32(&values, scalar_val)
        );
        assert_eq!(
            le_f32(&values, scalar_val),
            scalar::le_f32(&values, scalar_val)
        );
        assert_eq!(
            gt_f32(&values, scalar_val),
            scalar::gt_f32(&values, scalar_val)
        );
        assert_eq!(
            ge_f32(&values, scalar_val),
            scalar::ge_f32(&values, scalar_val)
        );
    }

    #[test]
    fn f32_nan_matches_ieee() {
        // IEEE ordered comparison: any NaN operand collapses lt/le/gt/ge
        // to false, eq to false, ne to true — the reference behaviour
        // the scalar kernel encodes.
        let values = vec![f32::NAN, 1.0_f32, f32::NAN, 2.0_f32];
        assert_eq!(eq_f32(&values, f32::NAN), vec![0u64]);
        assert_eq!(lt_f32(&values, 10.0_f32), scalar::lt_f32(&values, 10.0));
        assert_eq!(ne_f32(&values, 1.0_f32), scalar::ne_f32(&values, 1.0));
    }

    #[test]
    fn f32_length_corner_cases() {
        for &n in &[0usize, 1, 7, 8, 15, 16, 63, 64, 255, 256] {
            let values: Vec<f32> = (0..n as u32).map(|i| i as f32).collect();
            let got = lt_f32(&values, 50.0);
            let expected = scalar::lt_f32(&values, 50.0);
            assert_eq!(got, expected, "lt_f32 len={n}");
        }
    }
}
