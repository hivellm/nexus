//! Bitmap kernels with runtime SIMD dispatch.
//!
//! Two primitives:
//!
//! - `popcount(words)` — `Σ popcnt(word_i)` over a `&[u64]`.
//! - `and_popcount(a, b)` — `Σ popcnt(a_i & b_i)`, used to compute
//!   set intersection cardinality from packed-bit adjacency (the
//!   formulation of `graph::algorithms::traversal::cosine_similarity`
//!   that we replace in a later commit).
//!
//! The x86 AVX2 path uses Mula's nibble-LUT popcount over 32 bytes
//! per iteration. The AVX-512 path uses `_mm512_popcnt_epi64` when the
//! CPU advertises `avx512vpopcntdq` (Zen 4, Ice Lake+), processing
//! eight `u64` words per cycle. NEON uses `vcntq_u8` + `vaddvq_u8`
//! over 16 bytes per iteration. Scalar fallback is already in
//! [`super::scalar`].

use std::sync::OnceLock;

use super::dispatch::cpu;
use super::scalar;

// ── dispatch wrappers ─────────────────────────────────────────────────────────

type PopcountFn = unsafe fn(&[u64]) -> u64;
static POPCOUNT: OnceLock<PopcountFn> = OnceLock::new();

type AndPopcountFn = unsafe fn(&[u64], &[u64]) -> u64;
static AND_POPCOUNT: OnceLock<AndPopcountFn> = OnceLock::new();

fn pick_popcount() -> PopcountFn {
    let features = cpu();
    if features.disabled {
        return wrap_scalar_popcount;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512_vpopcntdq {
            return x86::popcount_u64_avx512;
        }
        if features.avx2 {
            return x86::popcount_u64_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return aarch64_mod::popcount_u64_neon;
        }
    }
    wrap_scalar_popcount
}

fn pick_and_popcount() -> AndPopcountFn {
    let features = cpu();
    if features.disabled {
        return wrap_scalar_and_popcount;
    }
    #[cfg(target_arch = "x86_64")]
    {
        if features.avx512_vpopcntdq {
            return x86::and_popcount_u64_avx512;
        }
        if features.avx2 {
            return x86::and_popcount_u64_avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if features.neon {
            return aarch64_mod::and_popcount_u64_neon;
        }
    }
    wrap_scalar_and_popcount
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_scalar_popcount(words: &[u64]) -> u64 {
    scalar::popcount_u64(words)
}

/// # Safety
/// Adapter; always sound.
unsafe fn wrap_scalar_and_popcount(a: &[u64], b: &[u64]) -> u64 {
    scalar::and_popcount_u64(a, b)
}

/// Safe dispatch entry for popcount over a u64 slice.
#[inline]
pub fn popcount_u64(words: &[u64]) -> u64 {
    let kernel = POPCOUNT.get_or_init(pick_popcount);
    // SAFETY: `pick_popcount` only returns kernels whose target_feature
    // is verified present at runtime via `cpu()`.
    unsafe { kernel(words) }
}

/// Safe dispatch entry for AND-popcount (|A ∩ B| over packed bits).
#[inline]
pub fn and_popcount_u64(a: &[u64], b: &[u64]) -> u64 {
    assert_eq!(
        a.len(),
        b.len(),
        "simd::bitmap::and_popcount_u64: length mismatch"
    );
    let kernel = AND_POPCOUNT.get_or_init(pick_and_popcount);
    // SAFETY: see `popcount_u64` above.
    unsafe { kernel(a, b) }
}

/// Kernel tier names for observability.
pub fn kernel_tiers() -> [(&'static str, &'static str); 2] {
    let _ = POPCOUNT.get_or_init(pick_popcount);
    let _ = AND_POPCOUNT.get_or_init(pick_and_popcount);
    let features = cpu();
    let tier = if features.disabled {
        "scalar (NEXUS_SIMD_DISABLE)"
    } else if features.avx512_vpopcntdq {
        "avx512vpopcntdq"
    } else if features.avx2 {
        "avx2"
    } else if features.neon {
        "neon"
    } else {
        "scalar"
    };
    [("popcount_u64", tier), ("and_popcount_u64", tier)]
}

// ── x86 kernels ───────────────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
mod x86 {
    use std::arch::x86_64::*;

    // Mula's nibble-LUT for popcount, replicated across both 128-bit
    // lanes of the 256-bit vector so `_mm256_shuffle_epi8` can do two
    // parallel lookups per instruction.
    #[inline]
    unsafe fn popcnt_lookup_table() -> __m256i {
        _mm256_setr_epi8(
            0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4, 0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2,
            3, 3, 4,
        )
    }

    /// Popcount an aligned run of 32 bytes (4 × u64) using the nibble LUT.
    ///
    /// # Safety
    /// Caller must ensure AVX2 is available.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn popcnt_avx2_chunk(v: __m256i) -> __m256i {
        let lookup = popcnt_lookup_table();
        let low_mask = _mm256_set1_epi8(0x0f);
        let low = _mm256_and_si256(v, low_mask);
        let high = _mm256_and_si256(_mm256_srli_epi16::<4>(v), low_mask);
        let lo_cnt = _mm256_shuffle_epi8(lookup, low);
        let hi_cnt = _mm256_shuffle_epi8(lookup, high);
        _mm256_add_epi8(lo_cnt, hi_cnt)
    }

    /// Fold the per-byte counts produced by `popcnt_avx2_chunk` into
    /// a single 64-bit sum via `_mm256_sad_epu8`.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn horizontal_sum_epi64(v: __m256i) -> u64 {
        let sad = _mm256_sad_epu8(v, _mm256_setzero_si256());
        // sad now holds four 64-bit partial sums (one per 64-bit lane).
        let low = _mm256_castsi256_si128(sad);
        let high = _mm256_extracti128_si256::<1>(sad);
        let combined = _mm_add_epi64(low, high);
        // combined: [a, b], sum = a + b
        let shifted = _mm_shuffle_epi32::<0b11_10>(combined); // move high qword to low
        let total = _mm_add_epi64(combined, shifted);
        _mm_cvtsi128_si64(total) as u64
    }

    /// # Safety
    /// Host must advertise `avx2`.
    #[target_feature(enable = "avx2")]
    pub unsafe fn popcount_u64_avx2(words: &[u64]) -> u64 {
        let mut acc_bytes = _mm256_setzero_si256();
        let mut total: u64 = 0;
        let len = words.len();
        let ptr = words.as_ptr() as *const __m256i;

        // Process 4 × u64 (= 32 bytes) per iteration. Flush the byte
        // accumulator to the 64-bit running total every 256 iterations
        // to avoid overflow (max byte value = 8, 32 lanes per vector,
        // 256 × 256 = 64k fits in u8).
        let chunks = len / 4;
        let flush_every = 255;
        let mut since_flush = 0usize;
        for i in 0..chunks {
            let v = _mm256_loadu_si256(ptr.add(i));
            acc_bytes = _mm256_add_epi8(acc_bytes, popcnt_avx2_chunk(v));
            since_flush += 1;
            if since_flush == flush_every {
                total += horizontal_sum_epi64(acc_bytes);
                acc_bytes = _mm256_setzero_si256();
                since_flush = 0;
            }
        }
        total += horizontal_sum_epi64(acc_bytes);

        for i in (chunks * 4)..len {
            total += words[i].count_ones() as u64;
        }
        total
    }

    /// # Safety
    /// Host must advertise `avx2`.
    #[target_feature(enable = "avx2")]
    pub unsafe fn and_popcount_u64_avx2(a: &[u64], b: &[u64]) -> u64 {
        debug_assert_eq!(a.len(), b.len());
        let mut acc_bytes = _mm256_setzero_si256();
        let mut total: u64 = 0;
        let len = a.len();
        let ap = a.as_ptr() as *const __m256i;
        let bp = b.as_ptr() as *const __m256i;

        let chunks = len / 4;
        let flush_every = 255;
        let mut since_flush = 0usize;
        for i in 0..chunks {
            let av = _mm256_loadu_si256(ap.add(i));
            let bv = _mm256_loadu_si256(bp.add(i));
            let anded = _mm256_and_si256(av, bv);
            acc_bytes = _mm256_add_epi8(acc_bytes, popcnt_avx2_chunk(anded));
            since_flush += 1;
            if since_flush == flush_every {
                total += horizontal_sum_epi64(acc_bytes);
                acc_bytes = _mm256_setzero_si256();
                since_flush = 0;
            }
        }
        total += horizontal_sum_epi64(acc_bytes);

        for i in (chunks * 4)..len {
            total += (a[i] & b[i]).count_ones() as u64;
        }
        total
    }

    /// # Safety
    /// Host must advertise `avx512f` and `avx512vpopcntdq`.
    #[target_feature(enable = "avx512f,avx512vpopcntdq")]
    pub unsafe fn popcount_u64_avx512(words: &[u64]) -> u64 {
        let mut acc = _mm512_setzero_si512();
        let len = words.len();
        let ptr = words.as_ptr() as *const __m512i;

        let chunks = len / 8;
        for i in 0..chunks {
            let v = _mm512_loadu_si512(ptr.add(i));
            acc = _mm512_add_epi64(acc, _mm512_popcnt_epi64(v));
        }
        let tail = len - chunks * 8;
        if tail > 0 {
            let mask: __mmask8 = (1u8 << tail) - 1;
            let v = _mm512_maskz_loadu_epi64(mask, words.as_ptr().add(chunks * 8) as *const i64);
            acc = _mm512_add_epi64(acc, _mm512_popcnt_epi64(v));
        }
        _mm512_reduce_add_epi64(acc) as u64
    }

    /// # Safety
    /// Host must advertise `avx512f` and `avx512vpopcntdq`.
    #[target_feature(enable = "avx512f,avx512vpopcntdq")]
    pub unsafe fn and_popcount_u64_avx512(a: &[u64], b: &[u64]) -> u64 {
        debug_assert_eq!(a.len(), b.len());
        let mut acc = _mm512_setzero_si512();
        let len = a.len();
        let ap = a.as_ptr() as *const __m512i;
        let bp = b.as_ptr() as *const __m512i;

        let chunks = len / 8;
        for i in 0..chunks {
            let av = _mm512_loadu_si512(ap.add(i));
            let bv = _mm512_loadu_si512(bp.add(i));
            let anded = _mm512_and_si512(av, bv);
            acc = _mm512_add_epi64(acc, _mm512_popcnt_epi64(anded));
        }
        let tail = len - chunks * 8;
        if tail > 0 {
            let mask: __mmask8 = (1u8 << tail) - 1;
            let av = _mm512_maskz_loadu_epi64(mask, a.as_ptr().add(chunks * 8) as *const i64);
            let bv = _mm512_maskz_loadu_epi64(mask, b.as_ptr().add(chunks * 8) as *const i64);
            let anded = _mm512_and_si512(av, bv);
            acc = _mm512_add_epi64(acc, _mm512_popcnt_epi64(anded));
        }
        _mm512_reduce_add_epi64(acc) as u64
    }
}

// ── aarch64 kernels ───────────────────────────────────────────────────────────

#[cfg(target_arch = "aarch64")]
mod aarch64_mod {
    use std::arch::aarch64::*;

    /// # Safety
    /// Host must advertise `neon`.
    #[target_feature(enable = "neon")]
    pub unsafe fn popcount_u64_neon(words: &[u64]) -> u64 {
        let mut total: u64 = 0;
        let len = words.len();
        let ptr = words.as_ptr() as *const u8;

        // 16 bytes per NEON register = 2 × u64 per iteration.
        let chunks = len / 2;
        for i in 0..chunks {
            let v = vld1q_u8(ptr.add(i * 16));
            let cnt = vcntq_u8(v);
            total += vaddlvq_u8(cnt) as u64;
        }
        for i in (chunks * 2)..len {
            total += words[i].count_ones() as u64;
        }
        total
    }

    /// # Safety
    /// Host must advertise `neon`.
    #[target_feature(enable = "neon")]
    pub unsafe fn and_popcount_u64_neon(a: &[u64], b: &[u64]) -> u64 {
        debug_assert_eq!(a.len(), b.len());
        let mut total: u64 = 0;
        let len = a.len();
        let ap = a.as_ptr() as *const u8;
        let bp = b.as_ptr() as *const u8;

        let chunks = len / 2;
        for i in 0..chunks {
            let av = vld1q_u8(ap.add(i * 16));
            let bv = vld1q_u8(bp.add(i * 16));
            let anded = vandq_u8(av, bv);
            total += vaddlvq_u8(vcntq_u8(anded)) as u64;
        }
        for i in (chunks * 2)..len {
            total += (a[i] & b[i]).count_ones() as u64;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_popcount_matches_scalar() {
        let words: Vec<u64> = (0..97).map(|i| 0xDEAD_BEEF ^ (i as u64 * 31)).collect();
        let dispatched = popcount_u64(&words);
        let scalar_result = scalar::popcount_u64(&words);
        assert_eq!(dispatched, scalar_result);
    }

    #[test]
    fn dispatch_and_popcount_matches_scalar() {
        let a: Vec<u64> = (0..97).map(|i| 0xAAAA_AAAA ^ (i as u64 * 11)).collect();
        let b: Vec<u64> = (0..97).map(|i| 0x5555_5555 ^ (i as u64 * 7)).collect();
        let dispatched = and_popcount_u64(&a, &b);
        let scalar_result = scalar::and_popcount_u64(&a, &b);
        assert_eq!(dispatched, scalar_result);
    }

    #[test]
    fn dispatch_handles_various_sizes() {
        for &len in &[
            0usize, 1, 3, 4, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 1024,
        ] {
            let a: Vec<u64> = (0..len).map(|i| 0x1357_9BDF ^ (i as u64)).collect();
            let b: Vec<u64> = (0..len).map(|i| 0x2468_ACE0 ^ (i as u64)).collect();
            assert_eq!(popcount_u64(&a), scalar::popcount_u64(&a), "len={len}");
            assert_eq!(
                and_popcount_u64(&a, &b),
                scalar::and_popcount_u64(&a, &b),
                "len={len}"
            );
        }
    }

    #[test]
    fn kernel_tiers_lists_both_ops() {
        let tiers = kernel_tiers();
        assert_eq!(tiers[0].0, "popcount_u64");
        assert_eq!(tiers[1].0, "and_popcount_u64");
        // Both ops share the same tier.
        assert_eq!(tiers[0].1, tiers[1].1);
    }
}
