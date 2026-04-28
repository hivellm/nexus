## 1. Implementation
- [ ] 1.1 Locate both `#[ignore]` markers in `crates/nexus-core/src/engine/tests.rs` that note default-data-dir parallel-test conflicts
- [ ] 1.2 Replace hard-coded paths with `tempfile::tempdir()` (per-test scoped)
- [ ] 1.3 Remove `#[ignore]` attributes
- [ ] 1.4 Run `cargo +nightly test -p nexus-core --lib` with default parallelism — confirm green
- [ ] 1.5 If `crates/nexus-core/src/testing/TestDatabase` exists and matches the pattern, refactor both tests to use it
- [ ] 1.6 Audit whole repo for other `#[ignore]` markers with the same placeholder-comment pattern; fix or escalate

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
