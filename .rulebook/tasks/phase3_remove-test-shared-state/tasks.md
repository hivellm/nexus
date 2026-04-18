## 1. Implementation
- [ ] 1.1 Introduce `struct TestExecutor { executor: Executor, _guard: tempfile::TempDir }` (or keep TempDir on Executor behind a `#[cfg(test)]` field)
- [ ] 1.2 Replace `impl Default for Executor` with `fn for_tests() -> TestExecutor` and update all test callers
- [ ] 1.3 Delete `SHARED_STORE`, `INIT`, and the `std::mem::forget(temp_dir)` call
- [ ] 1.4 Grep for other `static .*_STORE|static .*_ENGINE` under `nexus-core` and fix anywhere the same pattern appears
- [ ] 1.5 Run `cargo test -p nexus-core` with default parallelism and confirm tests pass + no test isolation warnings

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/TESTING.md` (or append to `AGENTS.md`) describing the new test helper pattern
- [ ] 2.2 Add a regression test that proves two parallel `Executor::for_tests()` instances do not share state (e.g. create a node in one, assert it's invisible from the other)
- [ ] 2.3 Run `cargo test --workspace` and confirm all pass
