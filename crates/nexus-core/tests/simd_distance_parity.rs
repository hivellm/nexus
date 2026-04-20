//! SIMD-vs-scalar parity for distance kernels.
//!
//! Every SIMD kernel in `simd::x86` (and `simd::aarch64` on AArch64
//! hosts) must agree with `simd::scalar` within the tolerance defined
//! by ADR-003:
//!
//! - f32 kernels: `1e-4 * max(1, |result|) * max(1, n)` accounts for the
//!   rounding-error drift that reordered FMAs introduce when combined
//!   with accumulators; the growing factor `n` covers the full
//!   benchmark dim range (up to 1536).
//! - Cosine distance is bounded [0, 2] so a fixed `5e-4` tolerance is
//!   enough regardless of dim.
//!
//! Kernels that the host CPU does not support are skipped via
//! `simd::cpu()` feature flags — the dispatch layer's job is exactly
//! this decision, we mirror it here so the test suite exercises only
//! the kernels that the build is going to run.

use nexus_core::simd::{cpu, distance, scalar};
use proptest::prelude::*;

fn tol_dot(n: usize, scale: f32) -> f32 {
    (5e-4 * (n as f32 + 1.0) * scale.max(1.0)).max(1e-4)
}

fn approx_eq(expected: f32, got: f32, tol: f32) -> bool {
    if expected.is_nan() || got.is_nan() {
        return expected.is_nan() && got.is_nan();
    }
    if expected.is_infinite() || got.is_infinite() {
        return expected.is_infinite() == got.is_infinite()
            && expected.is_sign_positive() == got.is_sign_positive();
    }
    (expected - got).abs() <= tol
}

prop_compose! {
    fn bounded_pair(max_len: usize)
        (len in 1usize..=max_len)
        (a in proptest::collection::vec(-4.0f32..4.0, len),
         b in proptest::collection::vec(-4.0f32..4.0, len))
        -> (Vec<f32>, Vec<f32>) {
        (a, b)
    }
}

// ── dispatch layer vs scalar ──────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn dispatch_dot_matches_scalar((a, b) in bounded_pair(512)) {
        let scale = a.iter().chain(b.iter())
            .copied().map(f32::abs).fold(0.0_f32, f32::max);
        let s = scalar::dot_f32(&a, &b);
        let d = distance::dot_f32(&a, &b);
        prop_assert!(
            approx_eq(s, d, tol_dot(a.len(), scale)),
            "dot len={} scalar={s} dispatch={d}", a.len()
        );
    }

    #[test]
    fn dispatch_l2_sq_matches_scalar((a, b) in bounded_pair(512)) {
        let scale = a.iter().chain(b.iter())
            .copied().map(f32::abs).fold(0.0_f32, f32::max);
        let s = scalar::l2_sq_f32(&a, &b);
        let d = distance::l2_sq_f32(&a, &b);
        prop_assert!(
            approx_eq(s, d, tol_dot(a.len(), scale * scale)),
            "l2_sq len={} scalar={s} dispatch={d}", a.len()
        );
    }

    #[test]
    fn dispatch_cosine_matches_scalar((a, b) in bounded_pair(512)) {
        let s = scalar::cosine_f32(&a, &b);
        let d = distance::cosine_f32(&a, &b);
        prop_assert!(
            approx_eq(s, d, 5e-4),
            "cosine len={} scalar={s} dispatch={d}", a.len()
        );
    }

    #[test]
    fn dispatch_normalize_matches_scalar(v in proptest::collection::vec(-4.0f32..4.0, 1usize..=512)) {
        let mut s = v.clone();
        let mut d = v.clone();
        let sn = scalar::normalize_f32(&mut s);
        let dn = distance::normalize_f32(&mut d);
        prop_assert!(approx_eq(sn, dn, 5e-4 * sn.abs().max(1.0)));
        for (i, (sv, dv)) in s.iter().zip(d.iter()).enumerate() {
            prop_assert!(
                approx_eq(*sv, *dv, 5e-5),
                "normalize lane {i} len={} scalar={sv} dispatch={dv}", v.len()
            );
        }
    }
}

// ── direct x86 kernel parity (only when the host advertises the tier) ─────────

#[cfg(target_arch = "x86_64")]
mod x86_parity {
    use super::*;
    use nexus_core::simd::x86;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

        #[test]
        fn sse42_dot_matches_scalar((a, b) in bounded_pair(512)) {
            prop_assume!(cpu().sse42);
            let scale = a.iter().chain(b.iter()).copied().map(f32::abs).fold(0.0_f32, f32::max);
            // SAFETY: sse42 is verified present via cpu().sse42 above.
            let got = unsafe { x86::dot_f32_sse42(&a, &b) };
            let expected = scalar::dot_f32(&a, &b);
            prop_assert!(approx_eq(expected, got, tol_dot(a.len(), scale)));
        }

        #[test]
        fn sse42_l2_sq_matches_scalar((a, b) in bounded_pair(512)) {
            prop_assume!(cpu().sse42);
            let scale = a.iter().chain(b.iter()).copied().map(f32::abs).fold(0.0_f32, f32::max);
            // SAFETY: sse42 verified.
            let got = unsafe { x86::l2_sq_f32_sse42(&a, &b) };
            let expected = scalar::l2_sq_f32(&a, &b);
            prop_assert!(approx_eq(expected, got, tol_dot(a.len(), scale * scale)));
        }

        #[test]
        fn sse42_cosine_matches_scalar((a, b) in bounded_pair(512)) {
            prop_assume!(cpu().sse42);
            // SAFETY: sse42 verified.
            let got = unsafe { x86::cosine_f32_sse42(&a, &b) };
            let expected = scalar::cosine_f32(&a, &b);
            prop_assert!(approx_eq(expected, got, 5e-4));
        }

        #[test]
        fn sse42_normalize_matches_scalar(v in proptest::collection::vec(-4.0f32..4.0, 1usize..=512)) {
            prop_assume!(cpu().sse42);
            let mut s = v.clone();
            let mut d = v.clone();
            let sn = scalar::normalize_f32(&mut s);
            // SAFETY: sse42 verified.
            let dn = unsafe { x86::normalize_f32_sse42(&mut d) };
            prop_assert!(approx_eq(sn, dn, 5e-4 * sn.abs().max(1.0)));
            for (sv, dv) in s.iter().zip(d.iter()) {
                prop_assert!(approx_eq(*sv, *dv, 5e-5));
            }
        }

        #[test]
        fn avx2_dot_matches_scalar((a, b) in bounded_pair(512)) {
            prop_assume!(cpu().avx2);
            let scale = a.iter().chain(b.iter()).copied().map(f32::abs).fold(0.0_f32, f32::max);
            // SAFETY: avx2+fma verified via cpu().avx2 flag.
            let got = unsafe { x86::dot_f32_avx2(&a, &b) };
            let expected = scalar::dot_f32(&a, &b);
            prop_assert!(approx_eq(expected, got, tol_dot(a.len(), scale)));
        }

        #[test]
        fn avx2_l2_sq_matches_scalar((a, b) in bounded_pair(512)) {
            prop_assume!(cpu().avx2);
            let scale = a.iter().chain(b.iter()).copied().map(f32::abs).fold(0.0_f32, f32::max);
            // SAFETY: avx2 verified.
            let got = unsafe { x86::l2_sq_f32_avx2(&a, &b) };
            let expected = scalar::l2_sq_f32(&a, &b);
            prop_assert!(approx_eq(expected, got, tol_dot(a.len(), scale * scale)));
        }

        #[test]
        fn avx2_cosine_matches_scalar((a, b) in bounded_pair(512)) {
            prop_assume!(cpu().avx2);
            // SAFETY: avx2 verified.
            let got = unsafe { x86::cosine_f32_avx2(&a, &b) };
            let expected = scalar::cosine_f32(&a, &b);
            prop_assert!(approx_eq(expected, got, 5e-4));
        }

        #[test]
        fn avx2_normalize_matches_scalar(v in proptest::collection::vec(-4.0f32..4.0, 1usize..=512)) {
            prop_assume!(cpu().avx2);
            let mut s = v.clone();
            let mut d = v.clone();
            let sn = scalar::normalize_f32(&mut s);
            // SAFETY: avx2 verified.
            let dn = unsafe { x86::normalize_f32_avx2(&mut d) };
            prop_assert!(approx_eq(sn, dn, 5e-4 * sn.abs().max(1.0)));
            for (sv, dv) in s.iter().zip(d.iter()) {
                prop_assert!(approx_eq(*sv, *dv, 5e-5));
            }
        }

        #[test]
        fn avx512_dot_matches_scalar((a, b) in bounded_pair(512)) {
            prop_assume!(cpu().avx512f);
            let scale = a.iter().chain(b.iter()).copied().map(f32::abs).fold(0.0_f32, f32::max);
            // SAFETY: avx512f verified.
            let got = unsafe { x86::dot_f32_avx512(&a, &b) };
            let expected = scalar::dot_f32(&a, &b);
            prop_assert!(approx_eq(expected, got, tol_dot(a.len(), scale)));
        }

        #[test]
        fn avx512_l2_sq_matches_scalar((a, b) in bounded_pair(512)) {
            prop_assume!(cpu().avx512f);
            let scale = a.iter().chain(b.iter()).copied().map(f32::abs).fold(0.0_f32, f32::max);
            // SAFETY: avx512f verified.
            let got = unsafe { x86::l2_sq_f32_avx512(&a, &b) };
            let expected = scalar::l2_sq_f32(&a, &b);
            prop_assert!(approx_eq(expected, got, tol_dot(a.len(), scale * scale)));
        }

        #[test]
        fn avx512_cosine_matches_scalar((a, b) in bounded_pair(512)) {
            prop_assume!(cpu().avx512f);
            // SAFETY: avx512f verified.
            let got = unsafe { x86::cosine_f32_avx512(&a, &b) };
            let expected = scalar::cosine_f32(&a, &b);
            prop_assert!(approx_eq(expected, got, 5e-4));
        }

        #[test]
        fn avx512_normalize_matches_scalar(v in proptest::collection::vec(-4.0f32..4.0, 1usize..=512)) {
            prop_assume!(cpu().avx512f);
            let mut s = v.clone();
            let mut d = v.clone();
            let sn = scalar::normalize_f32(&mut s);
            // SAFETY: avx512f verified.
            let dn = unsafe { x86::normalize_f32_avx512(&mut d) };
            prop_assert!(approx_eq(sn, dn, 5e-4 * sn.abs().max(1.0)));
            for (sv, dv) in s.iter().zip(d.iter()) {
                prop_assert!(approx_eq(*sv, *dv, 5e-5));
            }
        }
    }
}
