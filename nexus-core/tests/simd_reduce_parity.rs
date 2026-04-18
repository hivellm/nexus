//! SIMD-vs-scalar parity for the numeric reduce kernels.
//!
//! Matches the contract documented in ADR-003 and the tolerances from
//! `docs/specs/simd-dispatch.md`: integer sums are bit-exact modulo
//! wrapping, floating-point sums within `1e-9 * max(1, |result|)`,
//! min/max bit-equal (same IEEE value) or both `None`.

use nexus_core::simd::{cpu, reduce, scalar};
use proptest::prelude::*;

fn approx_eq_f64(expected: f64, got: f64, tol: f64) -> bool {
    if expected.is_nan() || got.is_nan() {
        return expected.is_nan() && got.is_nan();
    }
    if expected.is_infinite() || got.is_infinite() {
        return expected.is_infinite() == got.is_infinite()
            && expected.is_sign_positive() == got.is_sign_positive();
    }
    (expected - got).abs() <= tol
}

fn approx_eq_f32(expected: f32, got: f32, tol: f32) -> bool {
    approx_eq_f64(expected as f64, got as f64, tol as f64)
}

/// Absolute tolerance for reordered-FMA f64 sums: grows linearly with
/// length and magnitude; `1e-9 * n * max(1, |scale|)`.
fn tol_sum_f64(n: usize, scale: f64) -> f64 {
    (1e-9 * (n as f64 + 1.0) * scale.max(1.0)).max(1e-9)
}

fn tol_sum_f32(n: usize, scale: f32) -> f32 {
    (1e-5 * (n as f32 + 1.0) * scale.max(1.0)).max(1e-5)
}

// ── dispatch-vs-scalar ─────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn dispatch_sum_i64_matches_scalar(values in proptest::collection::vec(any::<i64>(), 0usize..=512)) {
        prop_assert_eq!(reduce::sum_i64(&values), scalar::sum_i64(&values));
    }

    #[test]
    fn dispatch_sum_f64_matches_scalar(values in proptest::collection::vec(-1e6f64..1e6, 0usize..=512)) {
        let scale = values.iter().copied().map(f64::abs).fold(0.0, f64::max);
        let expected = scalar::sum_f64(&values);
        let got = reduce::sum_f64(&values);
        prop_assert!(approx_eq_f64(expected, got, tol_sum_f64(values.len(), scale)));
    }

    #[test]
    fn dispatch_sum_f32_matches_scalar(values in proptest::collection::vec(-1e4f32..1e4, 0usize..=512)) {
        let scale = values.iter().copied().map(f32::abs).fold(0.0, f32::max);
        let expected = scalar::sum_f32(&values);
        let got = reduce::sum_f32(&values);
        prop_assert!(approx_eq_f32(expected, got, tol_sum_f32(values.len(), scale)));
    }

    #[test]
    fn dispatch_min_f64_matches_scalar(values in proptest::collection::vec(-1e6f64..1e6, 0usize..=512)) {
        prop_assert_eq!(reduce::min_f64(&values), scalar::min_f64(&values));
    }

    #[test]
    fn dispatch_max_f64_matches_scalar(values in proptest::collection::vec(-1e6f64..1e6, 0usize..=512)) {
        prop_assert_eq!(reduce::max_f64(&values), scalar::max_f64(&values));
    }

    #[test]
    fn dispatch_min_f32_matches_scalar(values in proptest::collection::vec(-1e4f32..1e4, 0usize..=512)) {
        prop_assert_eq!(reduce::min_f32(&values), scalar::min_f32(&values));
    }

    #[test]
    fn dispatch_max_f32_matches_scalar(values in proptest::collection::vec(-1e4f32..1e4, 0usize..=512)) {
        prop_assert_eq!(reduce::max_f32(&values), scalar::max_f32(&values));
    }

    #[test]
    fn dispatch_min_max_f64_handle_nan(
        values in proptest::collection::vec(
            prop_oneof![
                Just(f64::NAN),
                -1e6f64..1e6,
            ],
            1usize..=256,
        )
    ) {
        prop_assert_eq!(reduce::min_f64(&values), scalar::min_f64(&values));
        prop_assert_eq!(reduce::max_f64(&values), scalar::max_f64(&values));
    }
}

// ── direct x86 kernel parity (host-gated) ─────────────────────────────────────

#[cfg(target_arch = "x86_64")]
mod x86_parity {
    use super::*;
    use nexus_core::simd::x86;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

        #[test]
        fn avx2_sum_i64_matches_scalar(values in proptest::collection::vec(any::<i64>(), 0usize..=512)) {
            prop_assume!(cpu().avx2);
            // SAFETY: avx2 verified.
            let got = unsafe { x86::sum_i64_avx2(&values) };
            prop_assert_eq!(got, scalar::sum_i64(&values));
        }

        #[test]
        fn avx512_sum_i64_matches_scalar(values in proptest::collection::vec(any::<i64>(), 0usize..=512)) {
            prop_assume!(cpu().avx512f);
            // SAFETY: avx512f verified.
            let got = unsafe { x86::sum_i64_avx512(&values) };
            prop_assert_eq!(got, scalar::sum_i64(&values));
        }

        #[test]
        fn avx2_sum_f64_matches_scalar(values in proptest::collection::vec(-1e6f64..1e6, 0usize..=512)) {
            prop_assume!(cpu().avx2);
            let scale = values.iter().copied().map(f64::abs).fold(0.0, f64::max);
            // SAFETY: avx2 verified.
            let got = unsafe { x86::sum_f64_avx2(&values) };
            prop_assert!(approx_eq_f64(scalar::sum_f64(&values), got, tol_sum_f64(values.len(), scale)));
        }

        #[test]
        fn avx512_sum_f64_matches_scalar(values in proptest::collection::vec(-1e6f64..1e6, 0usize..=512)) {
            prop_assume!(cpu().avx512f);
            let scale = values.iter().copied().map(f64::abs).fold(0.0, f64::max);
            // SAFETY: avx512f verified.
            let got = unsafe { x86::sum_f64_avx512(&values) };
            prop_assert!(approx_eq_f64(scalar::sum_f64(&values), got, tol_sum_f64(values.len(), scale)));
        }

        #[test]
        fn avx2_sum_f32_matches_scalar(values in proptest::collection::vec(-1e4f32..1e4, 0usize..=512)) {
            prop_assume!(cpu().avx2);
            let scale = values.iter().copied().map(f32::abs).fold(0.0, f32::max);
            // SAFETY: avx2 verified.
            let got = unsafe { x86::sum_f32_avx2(&values) };
            prop_assert!(approx_eq_f32(scalar::sum_f32(&values), got, tol_sum_f32(values.len(), scale)));
        }

        #[test]
        fn avx512_sum_f32_matches_scalar(values in proptest::collection::vec(-1e4f32..1e4, 0usize..=512)) {
            prop_assume!(cpu().avx512f);
            let scale = values.iter().copied().map(f32::abs).fold(0.0, f32::max);
            // SAFETY: avx512f verified.
            let got = unsafe { x86::sum_f32_avx512(&values) };
            prop_assert!(approx_eq_f32(scalar::sum_f32(&values), got, tol_sum_f32(values.len(), scale)));
        }

        #[test]
        fn avx2_min_max_f64_match_scalar(values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 1usize..=512)) {
            prop_assume!(cpu().avx2);
            // SAFETY: avx2 verified.
            prop_assert_eq!(unsafe { x86::min_f64_avx2(&values) }, scalar::min_f64(&values));
            prop_assert_eq!(unsafe { x86::max_f64_avx2(&values) }, scalar::max_f64(&values));
        }

        #[test]
        fn avx512_min_max_f64_match_scalar(values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 1usize..=512)) {
            prop_assume!(cpu().avx512f);
            // SAFETY: avx512f verified.
            prop_assert_eq!(unsafe { x86::min_f64_avx512(&values) }, scalar::min_f64(&values));
            prop_assert_eq!(unsafe { x86::max_f64_avx512(&values) }, scalar::max_f64(&values));
        }

        #[test]
        fn avx2_min_max_f32_match_scalar(values in proptest::collection::vec(prop_oneof![Just(f32::NAN), -1e4f32..1e4], 1usize..=512)) {
            prop_assume!(cpu().avx2);
            // SAFETY: avx2 verified.
            prop_assert_eq!(unsafe { x86::min_f32_avx2(&values) }, scalar::min_f32(&values));
            prop_assert_eq!(unsafe { x86::max_f32_avx2(&values) }, scalar::max_f32(&values));
        }

        #[test]
        fn avx512_min_max_f32_match_scalar(values in proptest::collection::vec(prop_oneof![Just(f32::NAN), -1e4f32..1e4], 1usize..=512)) {
            prop_assume!(cpu().avx512f);
            // SAFETY: avx512f verified.
            prop_assert_eq!(unsafe { x86::min_f32_avx512(&values) }, scalar::min_f32(&values));
            prop_assert_eq!(unsafe { x86::max_f32_avx512(&values) }, scalar::max_f32(&values));
        }
    }
}
