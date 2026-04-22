//! Criterion bench for `simd::rle::find_run_length` — the inner
//! loop of the adjacency-list RLE compressor.
//!
//! Three workload shapes span the distribution a graph compaction
//! actually sees:
//!
//! * `uniform`: every element equal (best case for the SIMD path;
//!   each iteration advances 4 or 8 elements)
//! * `grouped`: alternating runs of length 1–16 (realistic for
//!   adjacency lists of hub nodes)
//! * `unique`: strictly increasing (worst case — every call returns 1
//!   after a single SIMD compare)
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench simd_rle
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nexus_core::simd::rle as simd_rle;
use std::hint::black_box;

const SIZES: &[usize] = &[1_024, 16_384, 262_144];

fn make_uniform(n: usize) -> Vec<u64> {
    vec![0xDEAD_BEEF_u64; n]
}

fn make_grouped(n: usize) -> Vec<u64> {
    let mut out = Vec::with_capacity(n);
    let mut id = 0_u64;
    while out.len() < n {
        let run = 1 + (id % 16) as usize;
        for _ in 0..run {
            if out.len() == n {
                break;
            }
            out.push(id);
        }
        id += 1;
    }
    out
}

fn make_unique(n: usize) -> Vec<u64> {
    (0..n as u64).collect()
}

fn scan_all_scalar(values: &[u64]) -> u64 {
    let mut i = 0;
    let mut total = 0_u64;
    while i < values.len() {
        let run = simd_rle::find_run_length_scalar(values, i);
        total = total.wrapping_add(run as u64);
        i += run;
    }
    total
}

fn scan_all_dispatch(values: &[u64]) -> u64 {
    let mut i = 0;
    let mut total = 0_u64;
    while i < values.len() {
        let run = simd_rle::find_run_length(values, i);
        total = total.wrapping_add(run as u64);
        i += run;
    }
    total
}

fn bench_rle(c: &mut Criterion) {
    for &(label, generator) in [
        ("uniform", make_uniform as fn(usize) -> Vec<u64>),
        ("grouped", make_grouped),
        ("unique", make_unique),
    ]
    .iter()
    {
        let mut group = c.benchmark_group(format!("rle_scan_{label}"));
        for &n in SIZES {
            let data = generator(n);
            group.throughput(Throughput::Elements(n as u64));
            group.bench_with_input(BenchmarkId::new("scalar", n), &n, |b, _| {
                b.iter(|| scan_all_scalar(black_box(&data)))
            });
            group.bench_with_input(BenchmarkId::new("dispatch", n), &n, |b, _| {
                b.iter(|| scan_all_dispatch(black_box(&data)))
            });
        }
        group.finish();
    }
}

criterion_group!(benches, bench_rle);
criterion_main!(benches);
