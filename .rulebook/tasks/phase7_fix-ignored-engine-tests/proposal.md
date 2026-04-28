# Proposal: phase7_fix-ignored-engine-tests

## Why

`crates/nexus-core/src/engine/tests.rs` has two `#[ignore] // TODO: Fix - uses default data dir which conflicts with parallel tests` markers. The tests use a hard-coded `./data` or `/tmp` path, so when the workspace test runner spawns them in parallel they collide on the LMDB lock file and the WAL. Marking them `#[ignore]` was a tactical shortcut; the correct fix is to use `tempfile::tempdir()` per test so each `Engine` instance gets its own private directory. Today these tests don't run on CI, so any regression in the code paths they cover would not be caught. The fix is small (one day), the `tempfile` crate is already a workspace dep, and the rest of the test suite already follows this pattern.

## What Changes

- Replace the hard-coded paths with `tempfile::tempdir()` so each test owns its directory.
- Remove the `#[ignore]` attributes.
- Verify that running `cargo +nightly test --workspace -- --test-threads=N` with N > 1 still succeeds (currently the project notes some tests need `--test-threads=1`).
- If a more general pattern exists in `crates/nexus-core/src/testing/` (`TestDatabase` helper), refactor to use it.

## Impact

- Affected specs: none.
- Affected code: `crates/nexus-core/src/engine/tests.rs` (both ignored tests), possibly `crates/nexus-core/src/testing/mod.rs`.
- Breaking change: NO.
- User benefit: parallel test execution restored; CI catches regressions in those code paths.
