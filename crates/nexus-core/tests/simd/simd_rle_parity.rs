//! Byte-identical parity between the scalar RLE reference and the
//! SIMD-accelerated `compress_simd_rle`.
//!
//! The outer loop of the compressor stayed the same — only the inner
//! "how long is this run" step now goes through `simd::rle::
//! find_run_length`. The proptest below re-implements the scalar
//! encoder inline and compares its output byte-for-byte to the
//! production path, across typical adjacency-list shapes:
//!
//! * uniform — every entry identical (favours the run branch)
//! * strictly increasing — no runs at all (favours the literal branch)
//! * grouped — alternating runs of varying length (exercises both)
//! * random — purely pseudo-random, proptest-generated
//!
//! The output format is the one documented in
//! `storage::graph_engine::compression`: `0xFF + u64 + u16` for runs
//! of 3+ and `n (< 0x80) + n × u64` for literal blocks of up to 127.

use proptest::prelude::*;

/// Scalar reference implementation — byte-for-byte identical to the
/// pre-SIMD `compress_simd_rle` in compression.rs.
fn scalar_rle(rel_ids: &[u64]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rel_ids.len() {
        let target = rel_ids[i];
        let mut run = 1;
        while i + run < rel_ids.len() && rel_ids[i + run] == target {
            run += 1;
        }
        if run >= 3 {
            out.push(0xFF);
            out.extend_from_slice(&target.to_le_bytes());
            out.extend_from_slice(&(run as u16).to_le_bytes());
            i += run;
        } else {
            let lit = std::cmp::min(127, rel_ids.len() - i);
            out.push(lit as u8);
            for j in 0..lit {
                out.extend_from_slice(&rel_ids[i + j].to_le_bytes());
            }
            i += lit;
        }
    }
    out
}

/// Production path: `find_run_length` (SIMD-dispatched) inside the
/// same outer loop.
fn simd_rle(rel_ids: &[u64]) -> Vec<u8> {
    use nexus_core::simd::rle as simd_rle;
    let mut out = Vec::new();
    let mut i = 0;
    while i < rel_ids.len() {
        let target = rel_ids[i];
        let run = simd_rle::find_run_length(rel_ids, i);
        if run >= 3 {
            out.push(0xFF);
            out.extend_from_slice(&target.to_le_bytes());
            out.extend_from_slice(&(run as u16).to_le_bytes());
            i += run;
        } else {
            let lit = std::cmp::min(127, rel_ids.len() - i);
            out.push(lit as u8);
            for j in 0..lit {
                out.extend_from_slice(&rel_ids[i + j].to_le_bytes());
            }
            i += lit;
        }
    }
    out
}

#[test]
fn empty_input_produces_empty_output() {
    assert!(simd_rle(&[]).is_empty());
    assert!(scalar_rle(&[]).is_empty());
}

#[test]
fn single_element_produces_one_literal_block() {
    let rel_ids = vec![42u64];
    let simd = simd_rle(&rel_ids);
    assert_eq!(simd, scalar_rle(&rel_ids));
    assert_eq!(simd[0], 1);
}

#[test]
fn uniform_run_matches_scalar_across_lengths() {
    for &n in &[3usize, 4, 7, 8, 15, 16, 31, 32, 100, 1_000, 10_000] {
        let rel_ids = vec![0xCAFE_u64; n];
        assert_eq!(simd_rle(&rel_ids), scalar_rle(&rel_ids), "len={n}");
    }
}

#[test]
fn increasing_sequence_matches_scalar() {
    let rel_ids: Vec<u64> = (0..1_000).collect();
    assert_eq!(simd_rle(&rel_ids), scalar_rle(&rel_ids));
}

#[test]
fn grouped_runs_match_scalar() {
    let mut rel_ids = Vec::new();
    // 20 ones, 5 twos, 10 ones, 127 threes, 128 fours (spans literal
    // block boundary), 3 fives (exactly the run threshold)
    rel_ids.extend(vec![1u64; 20]);
    rel_ids.extend(vec![2u64; 5]);
    rel_ids.extend(vec![1u64; 10]);
    rel_ids.extend(vec![3u64; 127]);
    rel_ids.extend(vec![4u64; 128]);
    rel_ids.extend(vec![5u64; 3]);
    assert_eq!(simd_rle(&rel_ids), scalar_rle(&rel_ids));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn random_adjacency_bytes_identical(
        rel_ids in proptest::collection::vec(0u64..100, 0usize..=2_000),
    ) {
        prop_assert_eq!(simd_rle(&rel_ids), scalar_rle(&rel_ids));
    }

    #[test]
    fn fuzzed_large_ids_bytes_identical(
        rel_ids in proptest::collection::vec(any::<u64>(), 0usize..=1_000),
    ) {
        prop_assert_eq!(simd_rle(&rel_ids), scalar_rle(&rel_ids));
    }
}
