//! Criterion benches comparing SIMD distance kernels vs scalar
//! across the dim range Nexus actually sees in production (small
//! embeddings through BERT-class 768 and MiniLM 1024 up to 1536).
//!
//! Run with:
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench simd_distance
//! ```
//!
//! The output compares `scalar::<op>_f32` against
//! `simd::distance::<op>_f32` (which resolves to AVX-512 → AVX2 →
//! SSE4.2 → NEON → Scalar at runtime). The acceptance target per
//! ADR-003 and phase1 task 15.5 is ≥4× AVX2 vs scalar at dim=768.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nexus_core::simd::{distance, scalar};
use std::hint::black_box;

const DIMS: &[usize] = &[32, 128, 256, 512, 768, 1024, 1536];

fn make_vecs(dim: usize) -> (Vec<f32>, Vec<f32>) {
    let a: Vec<f32> = (0..dim).map(|i| ((i as f32) * 0.17).sin()).collect();
    let b: Vec<f32> = (0..dim).map(|i| ((i as f32) * 0.31).cos()).collect();
    (a, b)
}

fn bench_dot(c: &mut Criterion) {
    let mut group = c.benchmark_group("dot_f32");
    for &dim in DIMS {
        let (a, b) = make_vecs(dim);
        group.throughput(Throughput::Elements(dim as u64));
        group.bench_with_input(BenchmarkId::new("scalar", dim), &dim, |bencher, _| {
            bencher.iter(|| scalar::dot_f32(black_box(&a), black_box(&b)))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", dim), &dim, |bencher, _| {
            bencher.iter(|| distance::dot_f32(black_box(&a), black_box(&b)))
        });
    }
    group.finish();
}

fn bench_l2_sq(c: &mut Criterion) {
    let mut group = c.benchmark_group("l2_sq_f32");
    for &dim in DIMS {
        let (a, b) = make_vecs(dim);
        group.throughput(Throughput::Elements(dim as u64));
        group.bench_with_input(BenchmarkId::new("scalar", dim), &dim, |bencher, _| {
            bencher.iter(|| scalar::l2_sq_f32(black_box(&a), black_box(&b)))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", dim), &dim, |bencher, _| {
            bencher.iter(|| distance::l2_sq_f32(black_box(&a), black_box(&b)))
        });
    }
    group.finish();
}

fn bench_cosine(c: &mut Criterion) {
    let mut group = c.benchmark_group("cosine_f32");
    for &dim in DIMS {
        let (a, b) = make_vecs(dim);
        group.throughput(Throughput::Elements(dim as u64));
        group.bench_with_input(BenchmarkId::new("scalar", dim), &dim, |bencher, _| {
            bencher.iter(|| scalar::cosine_f32(black_box(&a), black_box(&b)))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", dim), &dim, |bencher, _| {
            bencher.iter(|| distance::cosine_f32(black_box(&a), black_box(&b)))
        });
    }
    group.finish();
}

fn bench_normalize(c: &mut Criterion) {
    let mut group = c.benchmark_group("normalize_f32");
    for &dim in DIMS {
        let (base, _) = make_vecs(dim);
        group.throughput(Throughput::Elements(dim as u64));
        group.bench_with_input(BenchmarkId::new("scalar", dim), &dim, |bencher, _| {
            bencher.iter_batched(
                || base.clone(),
                |mut v| scalar::normalize_f32(black_box(&mut v)),
                criterion::BatchSize::SmallInput,
            )
        });
        group.bench_with_input(BenchmarkId::new("dispatch", dim), &dim, |bencher, _| {
            bencher.iter_batched(
                || base.clone(),
                |mut v| distance::normalize_f32(black_box(&mut v)),
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_dot,
    bench_l2_sq,
    bench_cosine,
    bench_normalize
);
criterion_main!(benches);
