## 1. Implementation
- [x] 1.1 `AsyncWalStats` fields replaced with `AtomicU64`; a new `AsyncWalStatsSnapshot` plain-data struct is the owned view consumers receive via `snapshot()`
- [x] 1.2 Every `unsafe { ... &mut AsyncWalStats ... }` block at `async_wal.rs:146, 166, 223, 225, 335, 377` removed — writes now use `fetch_add` / `compare_exchange_weak`; reads use `load(Relaxed)`
- [x] 1.3 `stats()` returns `AsyncWalStatsSnapshot` built from per-field `.load(Ordering::Relaxed)`; `Engine::async_wal_stats` signature updated accordingly; `wal::mod` re-exports both `AsyncWalStats` and `AsyncWalStatsSnapshot`
- [x] 1.4 `append`, `flush`, `writer_thread` / `flush_batch` all use atomic `fetch_add` / `fetch_sub`-via-CAS-loop for the queue-depth counter with underflow protection, and atomic max-update via relaxed `compare_exchange_weak` loop
- [x] 1.5 `cargo +nightly clippy -p nexus-core --tests --benches -- -D warnings` clean — no new unsafe warnings; the 6 `unsafe` blocks the task originally called out are all gone

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation: module-level doc on `AsyncWalStats` explains the atomic-counter / snapshot-view model and references the pre-fix race (casting `Arc<AsyncWalStats>` through `*mut` aliased `&mut` across threads under the Rust memory model).
- [x] 2.2 Write tests covering the new behavior: regression test `concurrent_appends_count_exactly` drives 10 concurrent `append()` calls and asserts `entries_submitted == 10` — with the old pointer-cast implementation this count could come in below 10 under Miri / stressed loads.
- [x] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-core --lib wal` → 26 passed, 0 failed (26 = 25 previous + 1 new concurrent test).
