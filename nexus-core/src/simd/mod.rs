//! SIMD kernels with runtime CPU feature dispatch.
//!
//! SIMD is always compiled into `nexus-core`; when the host CPU reports
//! support for a faster instruction set the kernels are selected
//! automatically. No Cargo features gate the module — see ADR-003
//! "SIMD dispatch" for the design rationale.
//!
//! The module is organised around a single contract:
//!
//! 1. `scalar` holds reference implementations that are the ground truth.
//! 2. Architecture-specific submodules (`x86`, `aarch64`) hold the
//!    SIMD kernels, each gated by `#[cfg(target_arch = "...")]` and
//!    `#[target_feature(enable = "...")]` so they only execute on CPUs
//!    that advertise the feature.
//! 3. `dispatch` owns the `CpuFeatures` probe and the cascade that
//!    picks the best kernel at runtime, caching the selection in an
//!    `OnceLock` so the hot path pays no detection cost.
//!
//! Runtime escape hatch: setting `NEXUS_SIMD_DISABLE=1` in the process
//! environment forces every dispatch function to return the scalar
//! kernel. This is the emergency rollback lever called out in
//! ADR-003 and wired through `dispatch::cpu()`.
//!
//! Correctness: `proptest` suites assert that every SIMD kernel agrees
//! with its scalar reference within the tolerance documented in
//! `docs/specs/simd-dispatch.md` (`1e-5` absolute for f32, `1e-9 *
//! max(abs, 1)` for f64, bit-exact for integer/bitmap ops).

pub mod dispatch;
pub mod scalar;

pub use dispatch::{CpuFeatures, cpu};
