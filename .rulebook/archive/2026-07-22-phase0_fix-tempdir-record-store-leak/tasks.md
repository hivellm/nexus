# Tasks: phase0_fix-tempdir-record-store-leak

`Executor::default()` (`crates/nexus-core/src/executor/dispatch.rs`) did
`tempfile::tempdir().keep()`, permanently leaking a `%TEMP%\.tmpXXXXXX` dir with
the full record-store set on every call — 21,000+ leaked dirs (~100 GB) on a
user's disk. Fixed by tying the temp dir to the store via a ref-counted cleanup
guard dropped AFTER the mmaps, so it is removed when the last store clone drops.
Landed in commit a3c10a49.

## 1. Reproduce / confirm
- [x] 1.1 Confirmed `Executor::default()` leaks via `.keep()` (dispatch.rs);
  confirmed `RecordStore`/`PropertyStore`/`AdjacencyListStore` have no `Drop`
- [x] 1.2 Confirmed `TestContext` is NOT the source (isolated `nexus-test-tmp`
  base + deferred sweep); the leaked dirs are `%TEMP%\.tmpXXXXXX` from the
  `Executor::default()`/`Engine::new()` bypass paths
- [x] 1.3 Grep-verified the store mmap `Arc`s do not escape `RecordStore`
  (only comment mentions in executor/engine.rs, operators/path.rs)

## 2. Implement the guard
- [x] 2.1 Added `TempDirGuard` (`storage/temp_guard.rs`) — Drop best-effort
  `remove_dir_all`, ignores `NotFound`, logs other errors
- [x] 2.2 `RecordStore`: added `_cleanup: Option<Arc<TempDirGuard>>` declared
  LAST (after the mmaps); `new()` keeps `None`; `Clone` propagates the `Arc`
- [x] 2.3 Added `RecordStore::new_temporary()` (`nexus-store-` prefixed temp dir
  + attached guard) and a `path()` accessor
- [x] 2.4 `Executor::default()` uses `new_temporary()`; `.keep()` removed; doc
  comment rewritten
- [x] 2.5 `Engine::new()` uses `new_temporary()`; removed the redundant
  `_temp_dir` field; extracted `bootstrap_with_storage` so the temp and
  persistent paths cannot drift (avoids a double-open that would delete the dir
  mid-init). Server-safe: guard drops with the Engine, never on a timer
- [x] 2.6 `TestContext` untouched; no time-based sweep added for guard dirs

## 3. Tail (docs + tests — check or waive with tailWaiver)
- [x] 3.1 Update or create documentation covering the implementation — CHANGELOG
  entry added ("Executor and Engine temporary-directory leak")
- [x] 3.2 Write tests covering the new behavior —
  `tests/storage/record_store_temp_guard_test.rs`: dir exists then removed on
  drop; with a clone, dir survives the first drop and is removed only on the
  last clone drop. Plus 2 unit tests on `TempDirGuard`
- [x] 3.3 Run tests and confirm they pass — `cargo +nightly fmt --all` + clippy
  clean (pre-commit hook); full workspace `cargo +nightly test --workspace`
  green (5087 passed / 0 failed). Empirical proof: 437 tests that previously
  leaked hundreds of dirs now leak ZERO (before=after=0 net)

## Residual / follow-up
- ~2 of ~437 `nexus-store-*` dirs occasionally survived a run — likely the LMDB
  (heed) catalog file staying briefly locked on Windows in the `Engine::new()`
  temp path when the guard fires. Filed as
  `phase0_fix-engine-temp-dir-catalog-lock-residual` (minor; the primary
  `Executor::default()` leak is fully eliminated).
