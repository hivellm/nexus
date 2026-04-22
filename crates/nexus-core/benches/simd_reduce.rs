//! Criterion benches for the SIMD reduce kernels.
//!
//! Sizes span the scan lengths Cypher aggregates typically see in
//! Nexus: 64 elements (small star query), 1024 (medium WHERE), 16 384
//! and 262 144 (large analytics / full-scan reductions).
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench simd_reduce
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nexus_core::simd::{reduce, scalar};
use std::hint::black_box;

const SIZES: &[usize] = &[64, 1024, 16_384, 262_144];

fn make_i64(n: usize) -> Vec<i64> {
    (0..n).map(|i| (i as i64) * 7 - 100).collect()
}
fn make_f64(n: usize) -> Vec<f64> {
    (0..n).map(|i| ((i as f64) * 0.11).sin()).collect()
}
fn make_f32(n: usize) -> Vec<f32> {
    (0..n).map(|i| ((i as f32) * 0.17).cos()).collect()
}

fn bench_sum_i64(c: &mut Criterion) {
    let mut group = c.benchmark_group("sum_i64");
    for &n in SIZES {
        let data = make_i64(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("scalar", n), &n, |b, _| {
            b.iter(|| scalar::sum_i64(black_box(&data)))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", n), &n, |b, _| {
            b.iter(|| reduce::sum_i64(black_box(&data)))
        });
    }
    group.finish();
}

fn bench_sum_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("sum_f64");
    for &n in SIZES {
        let data = make_f64(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("scalar", n), &n, |b, _| {
            b.iter(|| scalar::sum_f64(black_box(&data)))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", n), &n, |b, _| {
            b.iter(|| reduce::sum_f64(black_box(&data)))
        });
    }
    group.finish();
}

fn bench_sum_f32(c: &mut Criterion) {
    let mut group = c.benchmark_group("sum_f32");
    for &n in SIZES {
        let data = make_f32(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("scalar", n), &n, |b, _| {
            b.iter(|| scalar::sum_f32(black_box(&data)))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", n), &n, |b, _| {
            b.iter(|| reduce::sum_f32(black_box(&data)))
        });
    }
    group.finish();
}

fn bench_min_max_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("min_max_f64");
    for &n in SIZES {
        let data = make_f64(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("min_scalar", n), &n, |b, _| {
            b.iter(|| scalar::min_f64(black_box(&data)))
        });
        group.bench_with_input(BenchmarkId::new("min_dispatch", n), &n, |b, _| {
            b.iter(|| reduce::min_f64(black_box(&data)))
        });
        group.bench_with_input(BenchmarkId::new("max_dispatch", n), &n, |b, _| {
            b.iter(|| reduce::max_f64(black_box(&data)))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_sum_i64,
    bench_sum_f64,
    bench_sum_f32,
    bench_min_max_f64
);
criterion_main!(benches);
