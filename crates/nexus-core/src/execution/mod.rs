//! Compiled Query Execution Engine
//!
//! Hosts the columnar / vectorized execution helpers used by the
//! planner's fast-path operators (`columnar::Column`,
//! `compiled::CompiledQuery`, `operators::VectorizedOperators`,
//! plus the SIMD-aware `ColumnarMemoryPool`). The JIT scaffold
//! that previously lived here was deleted in
//! `phase7_resolve-jit-module` (ADR
//! `delete-the-unused-jit-scaffold-rather-than-finish-the-cranelift-codegen`):
//! it had no production caller and the planner's bigger leverage
//! is cardinality propagation, not compiled queries.

pub mod columnar;
pub mod compiled;
pub mod integration_bench;
pub mod joins;
pub mod memory;
pub mod operators;
// `parallel` is intentionally not declared — the file exists but
// has been off the build since the same JIT-era refactor. If a
// future task wants parallel execution it should restart from
// scratch rather than re-enable the stale scaffold.

// Re-export main types
pub use columnar::{Column, ColumnarResult, DataType};
pub use compiled::{CompiledQuery, QueryCompiler};
pub use memory::ColumnarMemoryPool;
pub use operators::VectorizedOperators;
