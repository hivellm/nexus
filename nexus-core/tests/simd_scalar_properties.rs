//! Property-based parity tests for the scalar SIMD reference kernels.
//!
//! These tests guard the scalar implementations that every future SIMD
//! kernel is compared against. They encode the invariants documented
//! in ADR-003:
//!
//! * Integer / bitmap kernels: bit-exact against a hand-written
//!   reference (not `count_ones`, to catch transcription mistakes).
//! * f32 distance kernels: within `1e-5` absolute of a textbook
//!   formula, with extra tolerance on magnitude.
//!
//! When SIMD kernels land in later phases the same generators are
//! reused to compare SIMD output against the scalar reference — no
//! duplication required.

use nexus_core::simd::scalar;
use proptest::prelude::*;

/// Hand-written textbook dot product used as the ground truth.
fn reference_dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = 0.0_f32;
    for i in 0..a.len() {
        acc += a[i] * b[i];
    }
    acc
}

fn reference_l2_sq(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = 0.0_f32;
    for i in 0..a.len() {
        let d = a[i] - b[i];
        acc += d * d;
    }
    acc
}

fn reference_cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na * nb).sqrt();
    if denom == 0.0 { 0.0 } else { 1.0 - dot / denom }
}

fn reference_popcount(words: &[u64]) -> u64 {
    let mut total = 0_u64;
    for &w in words {
        let mut v = w;
        while v != 0 {
            total += (v & 1) as u64;
            v >>= 1;
        }
    }
    total
}

fn reference_and_popcount(a: &[u64], b: &[u64]) -> u64 {
    let mut total = 0_u64;
    for i in 0..a.len() {
        let mut v = a[i] & b[i];
        while v != 0 {
            total += (v & 1) as u64;
            v >>= 1;
        }
    }
    total
}

/// Allowed absolute error for f32 kernels, scaled to the input range.
///
/// `sum` and `dot` can accumulate ~`eps * n` rounding error for `n`
/// additions; using `1e-4 * (n + 1)` keeps tests stable up to the
/// largest generated length (256) without masking real drift.
fn tolerance_dot(n: usize, scale: f32) -> f32 {
    (1e-4 * (n as f32 + 1.0) * scale.max(1.0)).max(1e-5)
}

fn approx_eq_f32(expected: f32, got: f32, tol: f32) -> bool {
    if expected.is_nan() || got.is_nan() {
        expected.is_nan() && got.is_nan()
    } else if expected.is_infinite() || got.is_infinite() {
        expected.is_sign_positive() == got.is_sign_positive()
            && expected.is_infinite() == got.is_infinite()
    } else {
        (expected - got).abs() <= tol
    }
}

// Generates f32 slices in a bounded range so `na * nb` does not
// overflow to infinity in cosine tests at length 256.
prop_compose! {
    fn bounded_f32_pair(max_len: usize)
        (len in 1usize..=max_len)
        (a in proptest::collection::vec(-4.0f32..4.0, len),
         b in proptest::collection::vec(-4.0f32..4.0, len))
        -> (Vec<f32>, Vec<f32>) {
        (a, b)
    }
}

prop_compose! {
    fn u64_word_pair(max_len: usize)
        (len in 1usize..=max_len)
        (a in proptest::collection::vec(any::<u64>(), len),
         b in proptest::collection::vec(any::<u64>(), len))
        -> (Vec<u64>, Vec<u64>) {
        (a, b)
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn scalar_dot_matches_reference((a, b) in bounded_f32_pair(256)) {
        let scale = a.iter().chain(b.iter())
            .copied().map(f32::abs).fold(0.0_f32, f32::max);
        let expected = reference_dot(&a, &b);
        let got = scalar::dot_f32(&a, &b);
        prop_assert!(
            approx_eq_f32(expected, got, tolerance_dot(a.len(), scale)),
            "dot: expected {expected}, got {got}"
        );
    }

    #[test]
    fn scalar_l2_sq_matches_reference((a, b) in bounded_f32_pair(256)) {
        let scale = a.iter().chain(b.iter())
            .copied().map(f32::abs).fold(0.0_f32, f32::max);
        let expected = reference_l2_sq(&a, &b);
        let got = scalar::l2_sq_f32(&a, &b);
        prop_assert!(
            approx_eq_f32(expected, got, tolerance_dot(a.len(), scale * scale)),
            "l2_sq: expected {expected}, got {got}"
        );
    }

    #[test]
    fn scalar_cosine_matches_reference((a, b) in bounded_f32_pair(256)) {
        let expected = reference_cosine(&a, &b);
        let got = scalar::cosine_f32(&a, &b);
        // Cosine is bounded [0, 2] (when norms are non-zero) so a
        // fixed tolerance works regardless of vector magnitude.
        prop_assert!(
            approx_eq_f32(expected, got, 1e-4),
            "cosine: expected {expected}, got {got}"
        );
    }

    #[test]
    fn scalar_normalize_yields_unit_vector_or_leaves_zero(v in proptest::collection::vec(-4.0f32..4.0, 1usize..=256)) {
        let mut data = v.clone();
        let norm = scalar::normalize_f32(&mut data);
        if norm == 0.0 {
            prop_assert!(data.iter().all(|x| *x == 0.0));
        } else {
            let new_norm_sq: f32 = data.iter().map(|x| x * x).sum();
            prop_assert!(
                approx_eq_f32(1.0, new_norm_sq, 1e-4),
                "normalized vector has norm² {new_norm_sq}, pre-norm {norm}"
            );
        }
    }

    #[test]
    fn scalar_popcount_matches_reference(words in proptest::collection::vec(any::<u64>(), 1usize..=64)) {
        prop_assert_eq!(scalar::popcount_u64(&words), reference_popcount(&words));
    }

    #[test]
    fn scalar_and_popcount_matches_reference((a, b) in u64_word_pair(64)) {
        prop_assert_eq!(
            scalar::and_popcount_u64(&a, &b),
            reference_and_popcount(&a, &b)
        );
    }
}
