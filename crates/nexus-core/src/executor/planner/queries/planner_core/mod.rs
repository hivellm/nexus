//! `impl QueryPlanner` constructor, builder shims, and `plan_query` —
//! the top-level planning entry point.
//!
//! The implementation is split across focused submodules:
//! - `builder`  — constructor, builder shims, `hash_query`, `plan_query`
//! - `bound`    — `plan_query_bound` (the core single-segment planning pass)
//! - `segments` — `WITH → MATCH` segmented planning + pattern-variable
//!   collection

mod bound;
mod builder;
mod segments;
