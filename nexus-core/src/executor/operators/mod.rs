//! Physical operator execution split out of the executor monolith.
//!
//! Each submodule attaches an `impl Executor { … }` block against the
//! core type declared in `super::engine`. Operators live here; expression
//! evaluation lives in `super::eval`.

pub mod aggregate;
