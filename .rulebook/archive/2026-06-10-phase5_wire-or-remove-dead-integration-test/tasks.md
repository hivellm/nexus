# Tasks: phase5_wire-or-remove-dead-integration-test

## 1. Implementation
- [x] 1.1 Inventory tests in tests/integration_test.rs; diff against existing per-crate integration tests to quantify unique coverage
- [x] 1.2 Decide wire-vs-remove with the user (removal needs explicit authorization) — user chose REMOVE (2026-06-10), per design.md recommendation
- [x] 1.3 If wiring: move into the chosen crate's tests/, split into ≤1500-line feature-area files, fix imports, make all tests pass — N/A: decision was remove, wiring branch not taken
- [x] 1.4 If removing: delete the file after user authorization — `git rm tests/integration_test.rs` executed with explicit user authorization

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG.md "Removed" entry under [Unreleased]
- [x] 2.2 Write tests covering the new behavior — N/A: removal of a never-compiled file introduces no new behavior; retained coverage is `crates/nexus-core/tests/integration.rs` (the 15 live duplicates of the dead file's only compilable group)
- [x] 2.3 Run tests and confirm they pass — `cargo check -p nexus-core` clean; `cargo test -p nexus-core --test integration` 15/15 passed
