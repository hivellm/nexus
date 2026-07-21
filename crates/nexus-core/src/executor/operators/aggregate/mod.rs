//! Aggregation operators: `execute_aggregate`, the projection-aware variant,
//! parallel/sequential execution paths, and the alias resolver used by
//! aggregation result labelling.
//!
//! The implementation is split across focused submodules:
//! - `core`     — `execute_aggregate` / `execute_aggregate_with_projections`
//! - `alias`    — `aggregation_alias`
//! - `columnar` — columnar fast-path helpers (§4 SIMD reduce kernels)

mod alias;
mod columnar;
mod core;

// Re-export the types used by external callers so existing import paths
// (`crate::executor::operators::aggregate::…`) remain valid.
pub use serde_json::{Map, Value};
pub use std::collections::HashMap;

pub use crate::executor::types::Aggregation;

#[cfg(test)]
mod tests;
