//! Write-query execution: MERGE / SET / REMOVE / FOREACH / UNWIND-write.
//! Extracted from `engine/mod.rs`, then split further into focused
//! submodules. Public API surface is unchanged — callers still resolve
//! methods as `Engine::execute_write_query`, `Engine::apply_set_clause`,
//! etc. via Rust's multi-file `impl` blocks.
//!
//! # Sub-module layout
//!
//! | File                 | Contents                                                    |
//! |----------------------|--------------------------------------------------------------|
//! | `dispatch.rs`        | Clause-loop orchestration (linear + UNWIND-write) and the MATCH/WHERE resolution it uses to bind write-path variables |
//! | `merge.rs`           | MERGE (node + relationship), `ON CREATE`/`ON MATCH`, and the exact-edge lookup MERGE relies on |
//! | `properties.rs`      | SET / REMOVE / FOREACH, plus the relationship property-mutation primitives they (and MERGE) share |
//! | `return_builder.rs`  | Write-path RETURN row construction, including the complex-expression fallback into the full executor |

mod dispatch;
mod merge;
mod properties;
mod return_builder;
