//! Compiled Query Execution Engine
//!
//! This module implements a compiled, vectorized query execution engine
//! that replaces the interpreted AST-based approach with SIMD-accelerated,
//! JIT-compiled native code execution.

pub mod columnar;
pub mod compiled;
pub mod integration_bench;
pub mod jit;
pub mod joins;
pub mod memory;
pub mod operators;
// pub mod parallel; // TODO: Re-enable after core optimizations

// Re-export main types
pub use columnar::{Column, ColumnarResult, DataType};
pub use compiled::{CompiledQuery, QueryCompiler};
pub use jit::{JitRuntime, QueryHints};
pub use memory::ColumnarMemoryPool;
pub use operators::VectorizedOperators;
