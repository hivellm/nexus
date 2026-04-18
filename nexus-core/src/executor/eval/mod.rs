//! Expression evaluation split out of the executor monolith.
//!
//! - `projection` — the main expression evaluator used by Project/With/
//!   Aggregate/Filter; dispatches through literals, arithmetic,
//!   string/list/map ops, case, patterns, and built-in functions.
//!
//! Each submodule attaches an `impl Executor { … }` block extending the
//! core type declared in `super::engine`.

pub mod arithmetic;
pub mod predicate;
pub mod projection;
pub mod temporal;
