# Tasks: phase0_fix-engine-temp-dir-catalog-lock-residual

Residual tail of `phase0_fix-tempdir-record-store-leak`: ~2 of ~437
`Engine::new()` temp dirs occasionally survive on Windows because the LMDB
(heed) catalog / WAL / Tantivy handles are still locked when the store's
`Arc<TempDirGuard>` fires `remove_dir_all`. The primary `Executor::default()`
leak is already fixed; this closes the tail.

## 1. Reproduce / confirm
- [ ] 1.1 Write a test: `Engine::new()`, capture its data dir, drop the engine,
  assert the dir is removed. Confirm it occasionally fails on Windows (dir
  survives) — the current guard test only covers a bare `RecordStore`
- [ ] 1.2 Determine which handle blocks removal (heed `Env`, async WAL writer
  thread, or a Tantivy index) via logging in `TempDirGuard::drop`

## 2. Fix
- [ ] 2.1 Ensure the catalog `Env` is fully closed (`EnvCloser` /
  `prepare_for_closing`) and the async WAL writer joined BEFORE the store's
  `_cleanup` guard fires — adjust `Engine` field drop order or add explicit
  teardown as needed
- [ ] 2.2 Optionally add a short retry-with-backoff to `TempDirGuard::drop` for
  Windows lock errors (`PermissionDenied`), giving closing handles a moment
- [ ] 2.3 Make the §1 test pass reliably

## 3. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 3.1 Update or create documentation covering the implementation (CHANGELOG
  if user-visible)
- [ ] 3.2 Write tests covering the new behavior (the §1 Engine temp-dir removal
  test)
- [ ] 3.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green)

## Related
- `phase0_fix-tempdir-record-store-leak` (primary fix, a3c10a49);
  `project-test-tempdir-leak-heed-env` (heed env close mechanism)
