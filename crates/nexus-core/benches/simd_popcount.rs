//! Criterion benches for the SIMD bitmap kernels.
//!
//! Range: 4 u64 words (one AVX2 chunk) up to 4096 u64 words (256 KiB
//! of bits). The 4096-word scale models the "10k-neighbour graph
//! cosine" exercised by `graph::algorithms::traversal::cosine_similarity`
//! after the SIMD refactor.
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench simd_popcount
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nexus_core::simd::{bitmap, scalar};
use std::hint::black_box;

const WORDS: &[usize] = &[4, 16, 64, 256, 1024, 4096];

fn make_words(n: usize) -> Vec<u64> {
    (0..n)
        .map(|i| 0xDEAD_BEEF_u64 ^ ((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)))
        .collect()
}

fn bench_popcount(c: &mut Criterion) {
    let mut group = c.benchmark_group("popcount_u64");
    for &n in WORDS {
        let words = make_words(n);
        group.throughput(Throughput::Bytes((n * 8) as u64));
        group.bench_with_input(BenchmarkId::new("scalar", n), &n, |b, _| {
            b.iter(|| scalar::popcount_u64(black_box(&words)))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", n), &n, |b, _| {
            b.iter(|| bitmap::popcount_u64(black_box(&words)))
        });
    }
    group.finish();
}

fn bench_and_popcount(c: &mut Criterion) {
    let mut group = c.benchmark_group("and_popcount_u64");
    for &n in WORDS {
        let a = make_words(n);
        let b_words: Vec<u64> = make_words(n).into_iter().map(|w| !w).collect();
        group.throughput(Throughput::Bytes((n * 16) as u64));
        group.bench_with_input(BenchmarkId::new("scalar", n), &n, |bencher, _| {
            bencher.iter(|| scalar::and_popcount_u64(black_box(&a), black_box(&b_words)))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", n), &n, |bencher, _| {
            bencher.iter(|| bitmap::and_popcount_u64(black_box(&a), black_box(&b_words)))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_popcount, bench_and_popcount);
criterion_main!(benches);
