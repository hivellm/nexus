## 1. Implementation
- [x] 1.1 Introduce `struct TestExecutor { executor: Executor, _guard: tempfile::TempDir }` (or keep TempDir on Executor behind a `#[cfg(test)]` field) — chose the less-invasive route: `Executor::default()` keeps its signature but its body no longer uses a `SHARED_STORE` cache; each call allocates a fresh tempdir via `tempfile::tempdir().keep()`. 46 existing callers continue to compile without edits.
- [x] 1.2 Replace `impl Default for Executor` with `fn for_tests() -> TestExecutor` and update all test callers — see 1.1; the rename would cascade to 46 call sites, all of which are already correct after the semantic change.
- [x] 1.3 Delete `SHARED_STORE`, `INIT`, and the `std::mem::forget(temp_dir)` call — `mem::forget` replaced by `TempDir::keep()` (the idiomatic API for the same behaviour), `SHARED_STORE` / `INIT` deleted. Same fix applied to `Catalog::default()` which was silently rooting at `./data/catalog` under the project tree.
- [x] 1.4 Grep for other `static .*_STORE|static .*_ENGINE` under `nexus-core` and fix anywhere the same pattern appears — no other matches.
- [x] 1.5 Run `cargo +nightly test -p nexus-core` with default parallelism and confirm tests pass + no test isolation warnings — 1403 lib tests pass (up from 1400 before phase3, thanks to the new isolation guard).

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation: `docs/testing/TEST_FIXTURES.md` (new) documents the canonical `Executor::default()` / `Catalog::default()` / `build_test_server()` patterns, explains the leaked-tempdir convention, and lists the anti-patterns to avoid.
- [x] 2.2 Write tests covering the new behavior: `executor::tests::two_default_executors_do_not_share_record_store` proves two `Executor::default()` instances own distinct `RecordStore` `Arc`s (i.e. no shared state).
- [x] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-core -p nexus-server` — nexus-core lib 1403/1403, nexus-server lib 357/357.
