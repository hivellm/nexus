# Proposal: phase0_fix-engine-temp-dir-catalog-lock-residual

**Priority: LOW — a small residual of the temp-dir leak fix
(`phase0_fix-tempdir-record-store-leak`): a minority of `Engine::new()`
temporary directories occasionally survive on Windows because the LMDB (heed)
catalog file is still locked when the store's cleanup guard fires.** The primary
leak (`Executor::default()`'s `.keep()`) is fully eliminated; this is the tail.

## Why

`RecordStore::new_temporary()` attaches an `Arc<TempDirGuard>` that removes the
directory when the last store clone drops (after the mmaps unmap). For a bare
`RecordStore` this is airtight. But `Engine::new()` also puts an LMDB catalog
(`catalog.mdb` via `heed`), a WAL, and Tantivy indexes inside that same
directory. When the guard fires on Windows, if the heed environment (or an
async WAL / Tantivy handle) has not fully released its file locks yet,
`remove_dir_all` fails for the locked file and the guard logs + leaves the
directory (best-effort). Empirically ~2 of ~437 temp dirs survived a mixed test
run — small, but non-zero.

Root-cause candidates to confirm: (a) the heed `Env` needs
`prepare_for_closing()`/`EnvCloser` to run before the guard removes the dir
(see `catalog::store::EnvCloser`, `project-test-tempdir-leak-heed-env`);
(b) the async WAL writer thread or a Tantivy index handle outlives the `Engine`
drop briefly; (c) `Engine` field drop order does not guarantee the catalog/WAL
close before the store's `_cleanup` guard fires.

## What Changes

- Trace `Engine` drop order and confirm the catalog `Env` is fully closed (its
  `EnvCloser` run) and the async WAL writer joined BEFORE the store's
  `_cleanup` guard removes the directory. Adjust field order or add an explicit
  teardown step if not.
- Optionally make `TempDirGuard::drop` retry-with-backoff a couple of times on a
  Windows `PermissionDenied`/locked error before giving up, to absorb the brief
  window where a handle is still closing.
- Add a regression test that creates an `Engine::new()`, records its data dir,
  drops the engine, and asserts the directory is removed (the current guard test
  only covers a bare `RecordStore`, not the catalog/WAL/index combination).

## Impact

- Affected specs: none
- Affected code: `crates/nexus-core/src/engine/mod.rs` (drop order / teardown),
  `crates/nexus-core/src/storage/temp_guard.rs` (optional retry),
  `crates/nexus-core/src/catalog/store.rs` (`EnvCloser` interaction)
- Breaking change: NO — internal robustness improvement
- User benefit: even the rare `Engine::new()` temp dir is cleaned up on Windows,
  fully closing the accumulation tail
- Related: `phase0_fix-tempdir-record-store-leak` (the primary fix, landed in
  a3c10a49), `project-test-tempdir-leak-heed-env` (the heed-env close mechanism)
