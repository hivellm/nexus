## 1. Implementation
- [ ] 1.1 Replace counter fields in `AsyncWalStats` with `AtomicU64`/`AtomicUsize`
- [ ] 1.2 Remove all `unsafe { ... &mut AsyncWalStats ... }` blocks at `async_wal.rs:146, 166, 223, 225`
- [ ] 1.3 Update `stats()` to return an owned snapshot built via `.load(Ordering::Relaxed)`
- [ ] 1.4 Update `append`, `flush`, `writer_thread` to use `fetch_add` / `store`
- [ ] 1.5 Run `cargo clippy --workspace -- -D warnings` and confirm no new unsafe warnings

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Document the atomic-stats model in module-level `//!` docs of `async_wal.rs`
- [ ] 2.2 Add a test that drives 10 concurrent `append()` calls and asserts `entries_submitted == 10`
- [ ] 2.3 Run tests and confirm they pass under `cargo test --package nexus-core wal::async_wal`
