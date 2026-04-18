//! Criterion benches for the SIMD compare kernels.
//!
//! Produces the packed Vec<u64> selection bitmap the phase-2 filter
//! operator will consume. Sizes match `simd_reduce.rs`.
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench simd_compare
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use nexus_core::simd::{compare, scalar};

const SIZES: &[usize] = &[64, 1024, 16_384, 262_144];

fn make_i64(n: usize) -> Vec<i64> {
    (0..n).map(|i| (i as i64) % 100).collect()
}
fn make_f64(n: usize) -> Vec<f64> {
    (0..n).map(|i| (i as f64) * 0.001).collect()
}

fn bench_eq_i64(c: &mut Criterion) {
    let mut group = c.benchmark_group("eq_i64");
    for &n in SIZES {
        let data = make_i64(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("scalar", n), &n, |b, _| {
            b.iter(|| scalar::eq_i64(black_box(&data), 50))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", n), &n, |b, _| {
            b.iter(|| compare::eq_i64(black_box(&data), 50))
        });
    }
    group.finish();
}

fn bench_lt_i64(c: &mut Criterion) {
    let mut group = c.benchmark_group("lt_i64");
    for &n in SIZES {
        let data = make_i64(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("scalar", n), &n, |b, _| {
            b.iter(|| scalar::lt_i64(black_box(&data), 50))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", n), &n, |b, _| {
            b.iter(|| compare::lt_i64(black_box(&data), 50))
        });
    }
    group.finish();
}

fn bench_lt_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("lt_f64");
    for &n in SIZES {
        let data = make_f64(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("scalar", n), &n, |b, _| {
            b.iter(|| scalar::lt_f64(black_box(&data), 0.5))
        });
        group.bench_with_input(BenchmarkId::new("dispatch", n), &n, |b, _| {
            b.iter(|| compare::lt_f64(black_box(&data), 0.5))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_eq_i64, bench_lt_i64, bench_lt_f64);
criterion_main!(benches);
