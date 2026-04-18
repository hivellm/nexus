//! Scalar reference kernels.
//!
//! These are the ground truth every SIMD kernel is tested against.
//! All functions here are `#[inline(always)]` so the compiler can
//! fold them into the caller when the dispatcher selects the scalar
//! path. They also serve as the fallback for every non-SIMD target
//! architecture.
//!
//! The signatures match the future SIMD kernels exactly so the
//! dispatch layer can store a function pointer without adaptation.

// ── f32 distance primitives ───────────────────────────────────────────────────

/// Dot product of two equal-length f32 slices.
///
/// Panics in debug builds when `a.len() != b.len()`. Release builds
/// reach only as far as the shorter of the two — the dispatcher above
/// is expected to have validated the length.
#[inline(always)]
pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "dot_f32: length mismatch");
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Squared L2 distance: `Σ (a_i − b_i)²`.
///
/// Squared form avoids the sqrt in hot paths where the caller needs
/// only a relative ranking (KNN, clustering).
#[inline(always)]
pub fn l2_sq_f32(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "l2_sq_f32: length mismatch");
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum()
}

/// Cosine distance (`1 − cosθ`).
///
/// Returns `0.0` when either vector has zero norm (matches the
/// convention used by the existing `KnnIndex` helper it replaces).
#[inline(always)]
pub fn cosine_f32(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "cosine_f32: length mismatch");
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = (na * nb).sqrt();
    if denom == 0.0 { 0.0 } else { 1.0 - dot / denom }
}

/// L2 normalise `v` in place. Returns the pre-normalisation norm so
/// callers that want the scaling factor (e.g. KNN reciprocal-rank)
/// can reuse it. Vectors of zero norm are left unchanged.
#[inline(always)]
pub fn normalize_f32(v: &mut [f32]) -> f32 {
    let mut sq = 0.0_f32;
    for &x in v.iter() {
        sq += x * x;
    }
    let norm = sq.sqrt();
    if norm > 0.0 {
        let inv = 1.0 / norm;
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
    norm
}

// ── bitmap kernels ────────────────────────────────────────────────────────────

/// Population count over a slice of `u64` words.
#[inline(always)]
pub fn popcount_u64(words: &[u64]) -> u64 {
    words.iter().map(|w| w.count_ones() as u64).sum()
}

/// Pointwise `AND` followed by popcount: `|A ∩ B|` when the inputs
/// encode set membership as packed bits.
#[inline(always)]
pub fn and_popcount_u64(a: &[u64], b: &[u64]) -> u64 {
    debug_assert_eq!(a.len(), b.len(), "and_popcount_u64: length mismatch");
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x & y).count_ones() as u64)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_matches_hand_calculation() {
        let a = [1.0_f32, 2.0, 3.0];
        let b = [4.0_f32, 5.0, 6.0];
        assert!((dot_f32(&a, &b) - 32.0).abs() < 1e-6);
    }

    #[test]
    fn l2_sq_is_zero_for_identical_vectors() {
        let a = [1.5_f32, -0.25, 10.0, 0.0];
        assert!(l2_sq_f32(&a, &a) < 1e-6);
    }

    #[test]
    fn cosine_is_zero_for_parallel_vectors() {
        let a = [1.0_f32, 0.0, 0.0];
        let b = [2.0_f32, 0.0, 0.0];
        // identical direction, different magnitude → cosine distance 0
        assert!(cosine_f32(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_is_one_for_orthogonal_vectors() {
        let a = [1.0_f32, 0.0, 0.0];
        let b = [0.0_f32, 1.0, 0.0];
        assert!((cosine_f32(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_handles_zero_vector() {
        let a = [0.0_f32, 0.0, 0.0];
        let b = [1.0_f32, 2.0, 3.0];
        assert_eq!(cosine_f32(&a, &b), 0.0);
    }

    #[test]
    fn normalize_reports_pre_norm_and_produces_unit_vector() {
        let mut v = [3.0_f32, 4.0, 0.0];
        let norm = normalize_f32(&mut v);
        assert!((norm - 5.0).abs() < 1e-6);
        let new_norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((new_norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_leaves_zero_vector_untouched() {
        let mut v = [0.0_f32; 8];
        let norm = normalize_f32(&mut v);
        assert_eq!(norm, 0.0);
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn popcount_matches_builtin() {
        let words = [0u64, u64::MAX, 0xAAAA_AAAA_AAAA_AAAA, 1];
        // 0 + 64 + 32 + 1
        assert_eq!(popcount_u64(&words), 97);
    }

    #[test]
    fn and_popcount_counts_set_intersection() {
        let a = [0b1100_u64, 0xFFFF_FFFF_FFFF_FFFF];
        let b = [0b1010_u64, 0x0000_0000_FFFF_FFFF];
        // (1100 & 1010) = 1000 → popcount 1
        // (all-ones & low32) = low32 → popcount 32
        assert_eq!(and_popcount_u64(&a, &b), 33);
    }
}
