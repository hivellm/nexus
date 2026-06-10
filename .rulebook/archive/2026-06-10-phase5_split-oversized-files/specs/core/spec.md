# Source Module Structure Specification

## ADDED Requirements

### Requirement: Source File Size Limit
Every Rust source file in `crates/` and `tests/` SHALL remain at or below 1500 lines; files exceeding the limit MUST be decomposed into cohesive submodules whose parent `mod.rs` acts as a thin facade.

#### Scenario: Oversized file is split without behavior change
Given a Rust source file exceeding 1500 lines
When it is decomposed into submodules
Then the public API paths remain unchanged via `pub use` re-exports
And `cargo +nightly check --workspace` passes
And `cargo clippy --workspace --all-targets --all-features -- -D warnings` reports zero warnings
And the full workspace test suite passes with no reduction in test count

#### Scenario: Facade preserves external imports
Given downstream code importing items from the original module path
When the module is split into submodules
Then all existing `use` statements in the workspace compile without modification
