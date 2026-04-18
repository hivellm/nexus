//! Physical operator execution split out of the executor monolith.
//!
//! Each submodule attaches an `impl Executor { … }` block against the
//! core type declared in `super::engine`. Operators live here; expression
//! evaluation lives in `super::eval`.

pub mod admin;
pub mod aggregate;
pub mod create;
pub mod dispatch;
pub mod expand;
pub mod filter;
pub mod join;
pub mod path;
pub mod procedures;
pub mod project;
pub mod scan;
pub mod union;
pub mod unwind;
