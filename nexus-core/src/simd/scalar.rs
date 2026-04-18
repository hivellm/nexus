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

// ── reductions over numeric columns ───────────────────────────────────────────
//
// Executor-side aggregates (SUM/MIN/MAX/AVG/COUNT) land as batch
// reductions in `simd::reduce`. The scalar implementations below are
// the ground truth every SIMD kernel is tested against.
//
// NaN semantics for min/max (f64, f32): a NaN operand is ignored —
// the returned value is always the min/max of the non-NaN inputs.
// An all-NaN or empty slice returns `None`.

/// Sum of all `i64` elements. Wrapping addition matches Cypher integer
/// semantics (no overflow errors in the aggregate path).
#[inline(always)]
pub fn sum_i64(values: &[i64]) -> i64 {
    let mut acc: i64 = 0;
    for &v in values {
        acc = acc.wrapping_add(v);
    }
    acc
}

/// Sum of all `f64` elements.
#[inline(always)]
pub fn sum_f64(values: &[f64]) -> f64 {
    let mut acc = 0.0_f64;
    for &v in values {
        acc += v;
    }
    acc
}

/// Sum of all `f32` elements, promoting to f64 internally to match the
/// precision the executor currently delivers for numeric aggregates.
#[inline(always)]
pub fn sum_f32(values: &[f32]) -> f32 {
    let mut acc = 0.0_f32;
    for &v in values {
        acc += v;
    }
    acc
}

/// Minimum `i64` value, or `None` if the slice is empty.
#[inline(always)]
pub fn min_i64(values: &[i64]) -> Option<i64> {
    let mut iter = values.iter().copied();
    let first = iter.next()?;
    let mut acc = first;
    for v in iter {
        if v < acc {
            acc = v;
        }
    }
    Some(acc)
}

/// Maximum `i64` value, or `None` if the slice is empty.
#[inline(always)]
pub fn max_i64(values: &[i64]) -> Option<i64> {
    let mut iter = values.iter().copied();
    let first = iter.next()?;
    let mut acc = first;
    for v in iter {
        if v > acc {
            acc = v;
        }
    }
    Some(acc)
}

/// Minimum `f64` value. `NaN` operands are ignored; an empty or
/// all-NaN slice returns `None`.
#[inline(always)]
pub fn min_f64(values: &[f64]) -> Option<f64> {
    let mut acc: Option<f64> = None;
    for &v in values {
        if v.is_nan() {
            continue;
        }
        acc = Some(match acc {
            Some(a) if a <= v => a,
            _ => v,
        });
    }
    acc
}

/// Maximum `f64` value. `NaN` operands are ignored.
#[inline(always)]
pub fn max_f64(values: &[f64]) -> Option<f64> {
    let mut acc: Option<f64> = None;
    for &v in values {
        if v.is_nan() {
            continue;
        }
        acc = Some(match acc {
            Some(a) if a >= v => a,
            _ => v,
        });
    }
    acc
}

/// Minimum `f32` value. `NaN` operands are ignored.
#[inline(always)]
pub fn min_f32(values: &[f32]) -> Option<f32> {
    let mut acc: Option<f32> = None;
    for &v in values {
        if v.is_nan() {
            continue;
        }
        acc = Some(match acc {
            Some(a) if a <= v => a,
            _ => v,
        });
    }
    acc
}

/// Maximum `f32` value. `NaN` operands are ignored.
#[inline(always)]
pub fn max_f32(values: &[f32]) -> Option<f32> {
    let mut acc: Option<f32> = None;
    for &v in values {
        if v.is_nan() {
            continue;
        }
        acc = Some(match acc {
            Some(a) if a >= v => a,
            _ => v,
        });
    }
    acc
}

// ── compare kernels ──────────────────────────────────────────────────────────
//
// Each returns a packed bitmap (Vec<u64>, LSB first per word): bit
// `i` is `1` iff the predicate `values[i] <op> scalar` holds.
//
// f64 predicates use IEEE ordered semantics: any NaN operand makes
// the predicate false (including `eq` — NaN != NaN — and `lt`/`gt`).
// `ne` returns `true` for NaN vs anything, matching scalar f64 `!=`.

fn bitmap_bytes(len: usize) -> Vec<u64> {
    vec![0u64; len.div_ceil(64)]
}

#[inline(always)]
fn set_bit(out: &mut [u64], idx: usize) {
    out[idx / 64] |= 1u64 << (idx % 64);
}

macro_rules! scalar_cmp_i64 {
    ($name:ident, $op:tt) => {
        #[inline(always)]
        pub fn $name(values: &[i64], scalar: i64) -> Vec<u64> {
            let mut out = bitmap_bytes(values.len());
            for (i, &v) in values.iter().enumerate() {
                if v $op scalar {
                    set_bit(&mut out, i);
                }
            }
            out
        }
    };
}
scalar_cmp_i64!(eq_i64, ==);
scalar_cmp_i64!(ne_i64, !=);
scalar_cmp_i64!(lt_i64, <);
scalar_cmp_i64!(le_i64, <=);
scalar_cmp_i64!(gt_i64, >);
scalar_cmp_i64!(ge_i64, >=);

macro_rules! scalar_cmp_f64 {
    ($name:ident, $op:tt) => {
        #[inline(always)]
        pub fn $name(values: &[f64], scalar: f64) -> Vec<u64> {
            let mut out = bitmap_bytes(values.len());
            for (i, &v) in values.iter().enumerate() {
                if v $op scalar {
                    set_bit(&mut out, i);
                }
            }
            out
        }
    };
}
scalar_cmp_f64!(eq_f64, ==);
scalar_cmp_f64!(ne_f64, !=);
scalar_cmp_f64!(lt_f64, <);
scalar_cmp_f64!(le_f64, <=);
scalar_cmp_f64!(gt_f64, >);
scalar_cmp_f64!(ge_f64, >=);

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
    fn sum_i64_matches_iter_sum() {
        let values = [1_i64, 2, 3, 4, 5, -10, 100];
        assert_eq!(sum_i64(&values), 105);
    }

    #[test]
    fn sum_f64_matches_iter_sum() {
        let values = [1.5_f64, 2.5, -3.0];
        assert!((sum_f64(&values) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn min_i64_returns_none_for_empty() {
        assert_eq!(min_i64(&[]), None);
        assert_eq!(max_i64(&[]), None);
    }

    #[test]
    fn min_max_i64_handle_extremes() {
        let values = [i64::MIN, -1, 0, 1, i64::MAX];
        assert_eq!(min_i64(&values), Some(i64::MIN));
        assert_eq!(max_i64(&values), Some(i64::MAX));
    }

    #[test]
    fn min_max_f64_ignore_nan() {
        let values = [f64::NAN, 1.5, f64::NAN, -2.5, 3.0];
        assert_eq!(min_f64(&values), Some(-2.5));
        assert_eq!(max_f64(&values), Some(3.0));
    }

    #[test]
    fn min_max_f64_all_nan_returns_none() {
        let values = [f64::NAN, f64::NAN];
        assert_eq!(min_f64(&values), None);
        assert_eq!(max_f64(&values), None);
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
