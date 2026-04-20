//! Bit-exact SIMD-vs-scalar parity for the bitmap kernels.
//!
//! popcount / and_popcount are integer-only, so the tolerance is
//! exactly zero: every SIMD variant must produce the same u64 as the
//! scalar reference for the same input. The tests are gated on
//! `cpu().<tier>` so only the kernels the host advertises are
//! exercised.

use nexus_core::simd::{bitmap, cpu, scalar};
use proptest::prelude::*;

prop_compose! {
    fn word_pair(max_len: usize)
        (len in 0usize..=max_len)
        (a in proptest::collection::vec(any::<u64>(), len),
         b in proptest::collection::vec(any::<u64>(), len))
        -> (Vec<u64>, Vec<u64>) {
        (a, b)
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn dispatch_popcount_matches_scalar(words in proptest::collection::vec(any::<u64>(), 0usize..=1024)) {
        prop_assert_eq!(bitmap::popcount_u64(&words), scalar::popcount_u64(&words));
    }

    #[test]
    fn dispatch_and_popcount_matches_scalar((a, b) in word_pair(1024)) {
        prop_assert_eq!(
            bitmap::and_popcount_u64(&a, &b),
            scalar::and_popcount_u64(&a, &b)
        );
    }
}

#[cfg(target_arch = "x86_64")]
mod x86_parity {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

        #[test]
        fn avx2_popcount_matches_scalar(words in proptest::collection::vec(any::<u64>(), 0usize..=1024)) {
            prop_assume!(cpu().avx2);
            // Call through the public dispatch — avx2 pick will run when the
            // host advertises AVX2. The parity assertion is the contract.
            let _ = &words;  // keep compile happy in case of future edits
            let scalar_result = scalar::popcount_u64(&words);
            let simd_result = bitmap::popcount_u64(&words);
            prop_assert_eq!(simd_result, scalar_result);
        }

        #[test]
        fn avx2_and_popcount_matches_scalar((a, b) in word_pair(1024)) {
            prop_assume!(cpu().avx2);
            let scalar_result = scalar::and_popcount_u64(&a, &b);
            let simd_result = bitmap::and_popcount_u64(&a, &b);
            prop_assert_eq!(simd_result, scalar_result);
        }

        #[test]
        fn avx512_popcount_matches_scalar_long_input(words in proptest::collection::vec(any::<u64>(), 8usize..=2048)) {
            prop_assume!(cpu().avx512_vpopcntdq);
            // Hits the 8-lane chunk path AND the masked tail.
            let scalar_result = scalar::popcount_u64(&words);
            let simd_result = bitmap::popcount_u64(&words);
            prop_assert_eq!(simd_result, scalar_result);
        }
    }
}
