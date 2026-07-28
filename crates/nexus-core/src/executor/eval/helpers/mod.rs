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
//! | `pattern_comprehension.rs` | Full-enumeration variant of `exists.rs`'s walk for pattern comprehensions — every complete binding is streamed through a caller-supplied projection closure instead of short-circuiting on the first witness |
//! | `tests`      | Unit coverage for `core.rs` / `exists.rs`, gated to test builds only |
//! | `pattern_comprehension_tests.rs` | Unit coverage for `pattern_comprehension.rs` and its parser lookahead, split out of `tests.rs` to stay under the 1500-line cap |

mod core;
mod exists;
mod pattern_comprehension;

#[cfg(test)]
mod pattern_comprehension_tests;
#[cfg(test)]
mod tests;
