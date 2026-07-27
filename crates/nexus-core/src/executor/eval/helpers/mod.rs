//! Evaluation helpers that sit between the operator layer and the
//! row-level evaluator. Split from the monolithic `helpers.rs` into a
//! directory module by cohesive concern; zero logic changes, only
//! `use`-path adjustments (one extra `super::` per relative import,
//! since every submodule now sits one level deeper) and doc-comment
//! relocation.
//!
//! # Sub-module layout
//!
//! | File         | Contents                                                                 |
//! |--------------|---------------------------------------------------------------------------|
//! | `core.rs`    | Cartesian-product application, row materialisation/update, entity id and relationship-value extraction, and the context expression evaluator |
//! | `exists.rs`  | The `EXISTS { … }` pattern-probe machinery: anchor resolution, candidate acceptance, and the (possibly variable-length) depth-first witness search |
//! | `tests`      | Unit coverage for both, gated to test builds only |

mod core;
mod exists;

#[cfg(test)]
mod tests;
