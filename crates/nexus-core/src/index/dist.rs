//! SIMD-backed distance kernels for the HNSW KNN index.
//!
//! Provides [`DistSimdCosine`] and [`DistSimdL2`] as concrete
//! [`hnsw_rs::dist::Distance`] implementations that dispatch to the
//! fastest SIMD kernel available on the host CPU.

use crate::simd;
use hnsw_rs::prelude::*;

/// Default dimensionality for `KnnIndex` when no per-call override is
/// supplied.
///
/// 128 matches the output dimension of the smallest of the HiveLLM
/// vectorizer profiles (see `hivellm/vectorizer`). It is also the
/// default MiniLM dim, so SDK examples that embed with a generic
/// off-the-shelf model round-trip without an explicit dimension
/// argument. Tests that exercise specific dimensions (64, 256, 768,
/// 1536) still pass their dimension explicitly to
/// [`crate::index::KnnIndex::new_default`] / [`crate::index::KnnIndex::new`]; this constant only
/// applies to the "give me a default index" path.
pub const DEFAULT_VECTORIZER_DIMENSION: usize = 128;

/// SIMD-backed cosine distance plugged into HNSW.
///
/// Delegates to `crate::simd::distance::cosine_f32`, which resolves
/// to the fastest kernel the host CPU supports (AVX-512 → AVX2 → SSE4.2
/// → NEON → Scalar). Produces bit-equivalent results to `DistSimdCosine`
/// within the `1e-4` tolerance documented in ADR-003.
#[derive(Default, Copy, Clone)]
pub struct DistSimdCosine;

impl Distance<f32> for DistSimdCosine {
    fn eval(&self, va: &[f32], vb: &[f32]) -> f32 {
        simd::distance::cosine_f32(va, vb)
    }
}

/// SIMD-backed squared L2 distance (Euclidean²) plugged into HNSW.
#[derive(Default, Copy, Clone)]
pub struct DistSimdL2;

impl Distance<f32> for DistSimdL2 {
    fn eval(&self, va: &[f32], vb: &[f32]) -> f32 {
        simd::distance::l2_sq_f32(va, vb)
    }
}
