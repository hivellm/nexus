# Tasks: phase0_fix-engine-temp-dir-catalog-lock-residual

Residual tail of `phase0_fix-tempdir-record-store-leak`: ~2 of ~437
`Engine::new()` temp dirs occasionally survive on Windows because the LMDB
(heed) catalog / WAL / Tantivy handles are still locked when the store's
`Arc<TempDirGuard>` fires `remove_dir_all`. The primary `Executor::default()`
leak is already fixed; this closes the tail.

> **RESCOPE (implementation):** profiling the actual on-disk accumulation
> (~4.5 GB, 700+ orphaned dirs in `%TEMP%`) showed the `Engine::new()`
> residual was negligible (1 dir). The DOMINANT leak was the HTTP server's
> `build_default_comparison_graphs`, which `tempfile::tempdir().keep()` +
> `RecordStore::new()`/`Catalog::new()`-leaked TWO dirs on every boot, plus
> `Catalog::default()` (reached in prod via the server's boot-time
> `Executor::default()`). The Engine residual is still closed (below), but
> the real fix is the server + catalog changes. ~4.5 GB of orphans were
> also swept from `%TEMP%`.

## 1. Reproduce / confirm
- [x] 1.1 Wrote `regression_engine_tempdir_removed_after_drop`
  (`tests/regression/regression_tests.rs`): `Engine::new()`, capture data dir,
  drop, assert removed (full catalog/WAL/index combination, not a bare store)
- [x] 1.2 Blocking handles identified: `Engine::new()` roots the LMDB catalog,
  WAL, and Tantivy index INSIDE the record-store temp dir; those fields drop
  AFTER `storage`, so the guard fired while they still held handles

## 2. Fix
- [x] 2.1 `Engine` gained a last-declared `_temp_dir_cleanup:
  Option<Arc<TempDirGuard>>` holding an independent clone of the store's guard,
  so removal is deferred until catalog/WAL/indexes/executor have all dropped
- [x] 2.2 `TempDirGuard::drop` retries `remove_dir_all` 5× with exponential
  backoff (10 ms → 160 ms) for transient Windows lock errors
- [x] 2.3 §1 test passes reliably
- [x] 2.4 (rescope) HTTP server: comparison graphs (`graph_a`/`graph_b`) are now
  lazily materialized on first `/comparison/*` request inside a self-removing
  `tempfile::TempDir` — an idle server allocates ZERO temp dirs; removed
  `build_default_comparison_graphs`; added `sweep_stale_comparison_dirs()` at
  startup for hard-kill orphans
- [x] 2.5 (rescope) `Catalog::default()` attaches an `Arc<TempDirGuard>`
  (prefix `nexus-catalog-`) instead of `.keep()`-leaking its LMDB dir

## 3. Tail (docs + tests — check or waive with tailWaiver)
- [x] 3.1 Update or create documentation covering the implementation — CHANGELOG
  entry added under `[3.0.0] — Unreleased`
- [x] 3.2 Write tests covering the new behavior —
  `regression_engine_tempdir_removed_after_drop`; existing comparison /
  temp-guard / record-store tests pass against the new code
- [x] 3.3 Run tests and confirm they pass — `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and
  `cargo +nightly test --workspace` (5235 passed / 0 failed / 96 ignored) all
  green; verified a full run leaves zero `nexus-cmp-*` / `nexus-store-*` /
  `nexus-catalog-*` dirs behind

## Related
- `phase0_fix-tempdir-record-store-leak` (primary fix, a3c10a49);
  `project-test-tempdir-leak-heed-env` (heed env close mechanism)
