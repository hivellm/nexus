//! Bit-exact SIMD-vs-scalar parity for the compare kernels.
//!
//! Output is a packed `Vec<u64>` — there is no tolerance: every SIMD
//! variant must produce the exact same bytes as the scalar reference
//! for the same input. Mixed-NaN float tests verify IEEE ordered-vs-
//! unordered predicate semantics match across all tiers.

use nexus_core::simd::{compare, cpu, scalar};
use proptest::prelude::*;

// ── dispatch-vs-scalar ─────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn dispatch_eq_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), scalar_v in any::<i64>()) {
        prop_assert_eq!(compare::eq_i64(&values, scalar_v), scalar::eq_i64(&values, scalar_v));
    }

    #[test]
    fn dispatch_ne_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), scalar_v in any::<i64>()) {
        prop_assert_eq!(compare::ne_i64(&values, scalar_v), scalar::ne_i64(&values, scalar_v));
    }

    #[test]
    fn dispatch_lt_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), scalar_v in any::<i64>()) {
        prop_assert_eq!(compare::lt_i64(&values, scalar_v), scalar::lt_i64(&values, scalar_v));
    }

    #[test]
    fn dispatch_le_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), scalar_v in any::<i64>()) {
        prop_assert_eq!(compare::le_i64(&values, scalar_v), scalar::le_i64(&values, scalar_v));
    }

    #[test]
    fn dispatch_gt_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), scalar_v in any::<i64>()) {
        prop_assert_eq!(compare::gt_i64(&values, scalar_v), scalar::gt_i64(&values, scalar_v));
    }

    #[test]
    fn dispatch_ge_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), scalar_v in any::<i64>()) {
        prop_assert_eq!(compare::ge_i64(&values, scalar_v), scalar::ge_i64(&values, scalar_v));
    }

    #[test]
    fn dispatch_eq_f64_with_nan(
        values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 0usize..=256),
        scalar_v in -1e6f64..1e6,
    ) {
        prop_assert_eq!(compare::eq_f64(&values, scalar_v), scalar::eq_f64(&values, scalar_v));
    }

    #[test]
    fn dispatch_ne_f64_with_nan(
        values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 0usize..=256),
        scalar_v in -1e6f64..1e6,
    ) {
        prop_assert_eq!(compare::ne_f64(&values, scalar_v), scalar::ne_f64(&values, scalar_v));
    }

    #[test]
    fn dispatch_lt_f64_with_nan(
        values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 0usize..=256),
        scalar_v in -1e6f64..1e6,
    ) {
        prop_assert_eq!(compare::lt_f64(&values, scalar_v), scalar::lt_f64(&values, scalar_v));
    }

    #[test]
    fn dispatch_le_f64_with_nan(
        values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 0usize..=256),
        scalar_v in -1e6f64..1e6,
    ) {
        prop_assert_eq!(compare::le_f64(&values, scalar_v), scalar::le_f64(&values, scalar_v));
    }

    #[test]
    fn dispatch_gt_f64_with_nan(
        values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 0usize..=256),
        scalar_v in -1e6f64..1e6,
    ) {
        prop_assert_eq!(compare::gt_f64(&values, scalar_v), scalar::gt_f64(&values, scalar_v));
    }

    #[test]
    fn dispatch_ge_f64_with_nan(
        values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 0usize..=256),
        scalar_v in -1e6f64..1e6,
    ) {
        prop_assert_eq!(compare::ge_f64(&values, scalar_v), scalar::ge_f64(&values, scalar_v));
    }
}

// ── direct x86 kernel parity ──────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
mod x86_parity {
    use super::*;
    use nexus_core::simd::x86;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

        #[test]
        fn avx2_eq_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), s in any::<i64>()) {
            prop_assume!(cpu().avx2);
            // SAFETY: avx2 verified.
            prop_assert_eq!(unsafe { x86::eq_i64_avx2(&values, s) }, scalar::eq_i64(&values, s));
        }

        #[test]
        fn avx512_eq_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), s in any::<i64>()) {
            prop_assume!(cpu().avx512f);
            // SAFETY: avx512f verified.
            prop_assert_eq!(unsafe { x86::eq_i64_avx512(&values, s) }, scalar::eq_i64(&values, s));
        }

        #[test]
        fn avx2_lt_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), s in any::<i64>()) {
            prop_assume!(cpu().avx2);
            // SAFETY: avx2 verified.
            prop_assert_eq!(unsafe { x86::lt_i64_avx2(&values, s) }, scalar::lt_i64(&values, s));
        }

        #[test]
        fn avx512_lt_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), s in any::<i64>()) {
            prop_assume!(cpu().avx512f);
            // SAFETY: avx512f verified.
            prop_assert_eq!(unsafe { x86::lt_i64_avx512(&values, s) }, scalar::lt_i64(&values, s));
        }

        #[test]
        fn avx2_gt_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), s in any::<i64>()) {
            prop_assume!(cpu().avx2);
            // SAFETY: avx2 verified.
            prop_assert_eq!(unsafe { x86::gt_i64_avx2(&values, s) }, scalar::gt_i64(&values, s));
        }

        #[test]
        fn avx512_gt_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), s in any::<i64>()) {
            prop_assume!(cpu().avx512f);
            // SAFETY: avx512f verified.
            prop_assert_eq!(unsafe { x86::gt_i64_avx512(&values, s) }, scalar::gt_i64(&values, s));
        }

        #[test]
        fn avx2_le_ge_ne_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), s in any::<i64>()) {
            prop_assume!(cpu().avx2);
            // SAFETY: avx2 verified.
            prop_assert_eq!(unsafe { x86::le_i64_avx2(&values, s) }, scalar::le_i64(&values, s));
            prop_assert_eq!(unsafe { x86::ge_i64_avx2(&values, s) }, scalar::ge_i64(&values, s));
            prop_assert_eq!(unsafe { x86::ne_i64_avx2(&values, s) }, scalar::ne_i64(&values, s));
        }

        #[test]
        fn avx512_le_ge_ne_i64(values in proptest::collection::vec(any::<i64>(), 0usize..=256), s in any::<i64>()) {
            prop_assume!(cpu().avx512f);
            // SAFETY: avx512f verified.
            prop_assert_eq!(unsafe { x86::le_i64_avx512(&values, s) }, scalar::le_i64(&values, s));
            prop_assert_eq!(unsafe { x86::ge_i64_avx512(&values, s) }, scalar::ge_i64(&values, s));
            prop_assert_eq!(unsafe { x86::ne_i64_avx512(&values, s) }, scalar::ne_i64(&values, s));
        }

        #[test]
        fn avx2_f64_all_ops(
            values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 0usize..=256),
            s in -1e6f64..1e6,
        ) {
            prop_assume!(cpu().avx2);
            // SAFETY: avx2 verified.
            prop_assert_eq!(unsafe { x86::eq_f64_avx2(&values, s) }, scalar::eq_f64(&values, s));
            prop_assert_eq!(unsafe { x86::ne_f64_avx2(&values, s) }, scalar::ne_f64(&values, s));
            prop_assert_eq!(unsafe { x86::lt_f64_avx2(&values, s) }, scalar::lt_f64(&values, s));
            prop_assert_eq!(unsafe { x86::le_f64_avx2(&values, s) }, scalar::le_f64(&values, s));
            prop_assert_eq!(unsafe { x86::gt_f64_avx2(&values, s) }, scalar::gt_f64(&values, s));
            prop_assert_eq!(unsafe { x86::ge_f64_avx2(&values, s) }, scalar::ge_f64(&values, s));
        }

        #[test]
        fn avx512_f64_all_ops(
            values in proptest::collection::vec(prop_oneof![Just(f64::NAN), -1e6f64..1e6], 0usize..=256),
            s in -1e6f64..1e6,
        ) {
            prop_assume!(cpu().avx512f);
            // SAFETY: avx512f verified.
            prop_assert_eq!(unsafe { x86::eq_f64_avx512(&values, s) }, scalar::eq_f64(&values, s));
            prop_assert_eq!(unsafe { x86::ne_f64_avx512(&values, s) }, scalar::ne_f64(&values, s));
            prop_assert_eq!(unsafe { x86::lt_f64_avx512(&values, s) }, scalar::lt_f64(&values, s));
            prop_assert_eq!(unsafe { x86::le_f64_avx512(&values, s) }, scalar::le_f64(&values, s));
            prop_assert_eq!(unsafe { x86::gt_f64_avx512(&values, s) }, scalar::gt_f64(&values, s));
            prop_assert_eq!(unsafe { x86::ge_f64_avx512(&values, s) }, scalar::ge_f64(&values, s));
        }
    }
}
