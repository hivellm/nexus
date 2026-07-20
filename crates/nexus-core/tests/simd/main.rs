//! Integration test harness for the `simd` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod simd_bitmap_parity;
mod simd_compare_parity;
mod simd_distance_parity;
mod simd_json_parity;
mod simd_reduce_parity;
mod simd_rle_parity;
mod simd_scalar_properties;
