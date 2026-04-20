//! Criterion bench comparing `crc32fast` (scalar IEEE) to
//! `simd::crc32c` (hardware Castagnoli via SSE4.2 / ARMv8 CRC).
//!
//! This is the direct throughput measurement that backs the WAL
//! dual-format migration: new v2 frames hash with CRC32C, legacy v1
//! frames keep `crc32fast` for backwards compatibility. Sizes span
//! typical WAL frame workloads — from a 256-byte BeginTx entry to a
//! 1 MiB bulk SetProperty.
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench simd_crc
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use nexus_core::simd::crc32c as simd_crc32c;

const SIZES: &[usize] = &[256, 4_096, 65_536, 1_048_576];

fn make_buf(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i as u8).wrapping_mul(31)).collect()
}

fn bench_crc(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc_checksum");
    for &n in SIZES {
        let data = make_buf(n);
        group.throughput(Throughput::Bytes(n as u64));
        group.bench_with_input(BenchmarkId::new("crc32fast_scalar", n), &n, |b, _| {
            b.iter(|| {
                let mut h = crc32fast::Hasher::new();
                h.update(black_box(&data));
                h.finalize()
            })
        });
        group.bench_with_input(BenchmarkId::new("crc32c_hardware", n), &n, |b, _| {
            b.iter(|| simd_crc32c::checksum(black_box(&data)))
        });
    }
    group.finish();
}

fn bench_iovecs(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc32c_iovecs");
    let algo = [0x02u8]; // ChecksumAlgo::Crc32C marker
    for &n in SIZES {
        let payload = make_buf(n);
        let type_buf = [0x10u8]; // CreateNode
        let len_buf = (payload.len() as u32).to_le_bytes();
        group.throughput(Throughput::Bytes(n as u64 + 6));
        group.bench_with_input(BenchmarkId::new("combine_4_slices", n), &n, |b, _| {
            b.iter(|| {
                simd_crc32c::checksum_iovecs(black_box(&[&algo, &type_buf, &len_buf, &payload]))
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_crc, bench_iovecs);
criterion_main!(benches);
