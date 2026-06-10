# Proposal: phase5_wire-or-remove-dead-integration-test

## Why
During phase5_split-oversized-files we discovered that `tests/integration_test.rs` (1892 lines, ~40+ tests) is dead code: the repository root is a virtual workspace (`[workspace]` only, no `[package]`), and no crate's Cargo.toml declares a `[[test]]` target pointing at it. The file is never compiled and never runs in CI — any regressions it would catch go undetected, and contributors reading it get a false sense of coverage.

## What Changes
Decide and implement one of:
1. **Wire it**: move the file into a member crate's `tests/` directory (likely `crates/nexus-core/tests/` or `crates/nexus-server/tests/`), fix imports, split it into ≤1500-line feature-area files (per the source-file size limit spec), and make all tests pass; or
2. **Remove it**: if its coverage is fully duplicated by the existing per-crate integration tests, delete it (requires explicit user authorization for deletion per Tier-1 rules).

Investigation first: diff its test inventory against existing per-crate tests to quantify unique coverage before choosing.

## Impact
- Affected specs: core (source-file size limit spec applies if wired)
- Affected code: tests/integration_test.rs, possibly crates/nexus-core/tests/ or crates/nexus-server/tests/
- Breaking change: NO
- User benefit: real (compiled, running) integration coverage or removal of misleading dead code
