//! Run-length scanning for the adjacency-list RLE compressor.
//!
//! `compress_simd_rle` in `storage::graph_engine::compression` was
//! originally named "SIMD-accelerated RLE" but implemented scalar
//! byte-at-a-time scanning. This module lands the actual SIMD path
//! behind the same output format: find the length of a run of equal
//! `u64` values in one vectorised read, so the outer compressor can
//! skip 4 or 8 elements per iteration instead of 1.
//!
//! Public entry point:
//!
//! ```ignore
//! let len = simd::rle::find_run_length(values, start);
//! ```
//!
//! Returns a length of at least 1 (the element at `start` itself
//! always counts).
//!
//! Output of the outer compressor is byte-identical to the scalar
//! version — proptest covers this in
//! `nexus-core/tests/simd_rle_parity.rs`.

use std::sync::OnceLock;

use super::dispatch::cpu;

type FindRunLengthFn = unsafe fn(&[u64], usize) -> usize;
static FIND_RUN_LENGTH: OnceLock<FindRunLengthFn> = OnceLock::new();

fn pick_find_run_length() -> FindRunLengthFn {
    let features = cpu();
    if features.disabled {
        return find_run_length_scalar_wrap;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512f {
            return x86::find_run_length_avx512;
        }
        if features.avx2 {
            return x86::find_run_length_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return aarch64_mod::find_run_length_neon;
        }
    }
    find_run_length_scalar_wrap
}

/// # Safety
/// Adapter; always sound.
unsafe fn find_run_length_scalar_wrap(values: &[u64], start: usize) -> usize {
    find_run_length_scalar(values, start)
}

/// Scalar reference. Returns the length of the run of equal values
/// starting at `values[start]`. Always `>= 1` when `start < values.len()`.
#[inline(always)]
pub fn find_run_length_scalar(values: &[u64], start: usize) -> usize {
    debug_assert!(start < values.len());
    let target = values[start];
    let mut len = 1;
    while start + len < values.len() && values[start + len] == target {
        len += 1;
    }
    len
}

/// Safe dispatch entry. Returns run length at least 1.
#[inline]
pub fn find_run_length(values: &[u64], start: usize) -> usize {
    assert!(
        start < values.len(),
        "simd::rle::find_run_length: start {start} >= len {}",
        values.len()
    );
    let kernel = FIND_RUN_LENGTH.get_or_init(pick_find_run_length);
    // SAFETY: only returns kernels whose target_feature is verified via cpu().
    unsafe { kernel(values, start) }
}

/// Kernel tier for `/stats`.
pub fn kernel_tier() -> &'static str {
    let _ = FIND_RUN_LENGTH.get_or_init(pick_find_run_length);
    cpu().preferred_tier()
}

// ── x86 ────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
mod x86 {
    use std::arch::x86_64::*;

    /// # Safety
    /// Host must advertise `avx2`.
    #[target_feature(enable = "avx2")]
    pub unsafe fn find_run_length_avx2(values: &[u64], start: usize) -> usize {
        debug_assert!(start < values.len());
        let target = _mm256_set1_epi64x(values[start] as i64);
        let mut len = 1;
        let base = values.as_ptr();
        while start + len + 4 <= values.len() {
            let v = _mm256_loadu_si256(base.add(start + len) as *const __m256i);
            let cmp = _mm256_cmpeq_epi64(v, target);
            let mask = _mm256_movemask_pd(_mm256_castsi256_pd(cmp)) as u32 & 0xF;
            if mask == 0xF {
                len += 4;
            } else {
                // Lanes below the first zero all match; `trailing_ones`
                // gives exactly that count.
                len += mask.trailing_ones() as usize;
                return len;
            }
        }
        // Scalar tail (< 4 remaining).
        let target_raw = values[start];
        while start + len < values.len() && values[start + len] == target_raw {
            len += 1;
        }
        len
    }

    /// # Safety
    /// Host must advertise `avx512f`.
    #[target_feature(enable = "avx512f")]
    pub unsafe fn find_run_length_avx512(values: &[u64], start: usize) -> usize {
        debug_assert!(start < values.len());
        let target = _mm512_set1_epi64(values[start] as i64);
        let mut len = 1;
        let base = values.as_ptr() as *const i64;
        while start + len + 8 <= values.len() {
            let v = _mm512_loadu_epi64(base.add(start + len));
            let mask: __mmask8 = _mm512_cmpeq_epi64_mask(v, target);
            if mask == 0xFF {
                len += 8;
            } else {
                len += (mask as u32).trailing_ones() as usize;
                return len;
            }
        }
        let target_raw = values[start];
        while start + len < values.len() && values[start + len] == target_raw {
            len += 1;
        }
        len
    }
}

// ── aarch64 ───────────────────────────────────────────────────────────────────

#[cfg(target_arch = "aarch64")]
mod aarch64_mod {
    use std::arch::aarch64::*;

    /// # Safety
    /// Host must advertise `neon`.
    #[target_feature(enable = "neon")]
    pub unsafe fn find_run_length_neon(values: &[u64], start: usize) -> usize {
        debug_assert!(start < values.len());
        let target = vdupq_n_u64(values[start]);
        let mut len = 1;
        let base = values.as_ptr();
        while start + len + 2 <= values.len() {
            let v = vld1q_u64(base.add(start + len));
            let cmp = vceqq_u64(v, target);
            let low = vgetq_lane_u64::<0>(cmp);
            let high = vgetq_lane_u64::<1>(cmp);
            if low == u64::MAX && high == u64::MAX {
                len += 2;
            } else if low == u64::MAX {
                len += 1;
                return len;
            } else {
                return len;
            }
        }
        let target_raw = values[start];
        while start + len < values.len() && values[start + len] == target_raw {
            len += 1;
        }
        len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_one_for_singleton_run() {
        let v = vec![7u64, 8, 9];
        assert_eq!(find_run_length(&v, 0), 1);
        assert_eq!(find_run_length(&v, 1), 1);
        assert_eq!(find_run_length(&v, 2), 1);
    }

    #[test]
    fn counts_long_run() {
        let v = vec![42u64; 100];
        assert_eq!(find_run_length(&v, 0), 100);
        assert_eq!(find_run_length(&v, 50), 50);
        assert_eq!(find_run_length(&v, 99), 1);
    }

    #[test]
    fn stops_at_boundary() {
        let mut v = vec![1u64; 20];
        v.extend(vec![2u64; 5]);
        v.extend(vec![1u64; 10]);
        assert_eq!(find_run_length(&v, 0), 20);
        assert_eq!(find_run_length(&v, 20), 5);
        assert_eq!(find_run_length(&v, 25), 10);
    }

    #[test]
    fn matches_scalar_on_various_inputs() {
        let cases = &[
            vec![1u64, 1, 1, 2, 2, 3, 3, 3, 3, 4],
            vec![42u64],
            vec![7u64; 1],
            (0..100_u64).collect::<Vec<_>>(),
            vec![0u64; 1000],
            {
                let mut v = vec![5u64; 17];
                v.extend(0..83_u64);
                v
            },
        ];
        for case in cases {
            for start in 0..case.len() {
                let expected = find_run_length_scalar(case, start);
                let got = find_run_length(case, start);
                assert_eq!(
                    got,
                    expected,
                    "case={:?} start={start} expected={expected} got={got}",
                    &case[..case.len().min(16)]
                );
            }
        }
    }
}
