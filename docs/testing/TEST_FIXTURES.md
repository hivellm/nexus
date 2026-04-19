# Test Fixtures

Guidance for writing tests in `nexus-core`, `nexus-server`, and
`nexus-protocol`. Keeps new tests parallel-safe by default and explains
when to step off the beaten path.

## Quick reference

| You want… | Use |
|-----------|-----|
| A throwaway `Executor` for a unit test | `Executor::default()` |
| A throwaway `Catalog` for a unit test | `Catalog::default()` |
| A `Catalog` rooted at a specific path | `Catalog::new(path)` |
| A `Catalog` that bypasses per-process sharing | `Catalog::with_isolated_path(path, map_size)` |
| A full `NexusServer` for an API handler test | `build_test_server()` (per-module helper, copy from any migrated API module — e.g. `nexus-server/src/api/data.rs::tests`) |
| An isolated `Engine` for a unit test | `Engine::with_isolated_catalog(path)` |
| A temp dir that outlives the test body | `nexus_core::testing::TestContext::new()` + `Box::leak` the context |

## Key invariants

### `Executor::default()` gives every caller fresh state

Before `phase3_remove-test-shared-state`, `Executor::default()` returned
a clone drawn from a process-wide `SHARED_STORE` — every test that
called `default()` observed every other test's writes. Post phase3:

- Each call allocates its own `tempfile::tempdir()`, keeps it via
  `TempDir::keep()` (the record store file descriptor stays valid for
  the lifetime of the process, but the directory is not cleaned up on
  `Executor` drop), and builds a fresh `RecordStore`, `Catalog`,
  `LabelIndex`, and `KnnIndex`.
- The leak is bounded by the number of `default()` calls in the test
  binary; acceptable in practice because test processes are short-lived.

If your test depends on two executors *sharing* state, that was the
bug — write one executor and give every assertion a read on it, or
accept the `Engine` / `Catalog` layer explicitly.

### `Catalog::default()` roots at a temp dir, not `./data/catalog`

Historically `Catalog::default()` created `./data/catalog` relative to
the current working directory. Under `cargo test --workspace` that
meant every test was hammering the same catalog *inside the project
root*, polluting the tree with stray `*.mdb` files and leaking label
IDs across tests. Post phase3, `Catalog::default()` uses
`tempfile::tempdir().keep()` and calls `Catalog::new(path)`; when
`Catalog::new` detects a cargo-test run it still folds the path into
the per-process `nexus_test_catalogs_shared` directory (to keep LMDB
TLS-slot usage bounded on Windows) but there is no longer a project-
tree side effect.

### `Catalog::new(path)` shares an LMDB env under `cargo test`

This is intentional — opening many LMDB environments on Windows
quickly exhausts TLS slots and fails with `TlsFull`. Every
`Catalog::new(path)` call under `cargo test` is silently redirected to
`<tmp>/nexus_test_catalogs_shared`. Tests that need **strict label-id
isolation** must call `Catalog::with_isolated_path(path, map_size)`
instead.

`nexus_core::Engine::with_isolated_catalog(path)` is the high-level
wrapper for "give me a brand-new isolated engine"; prefer it over
manually assembling the pieces.

### `nexus-server` API handler tests use a `build_test_server()` helper

Every API module under `nexus-server/src/api/` that was migrated in
`phase2a`–`phase2e` carries a `build_test_server()` fixture. The
canonical shape is documented once in
[`nexus-server/src/api/data.rs`](../../nexus-server/src/api/data.rs);
copy it verbatim into new test modules rather than depending across
modules — each test module's fixture stays self-contained so
`cargo test -p nexus-server --lib api::<module>` remains targeted.

### `TempDir` leaking

Throughout the codebase you will see the pattern:

```rust
let ctx = nexus_core::testing::TestContext::new();
let _leaked = Box::leak(Box::new(ctx));
```

inside a test fixture. This keeps the `TempDir` (and therefore the
catalog's backing directory) alive for the remainder of the process,
which matters because the fixture returns an `Arc<NexusServer>` whose
lifetime is unbounded from the outside. The leak is bounded by the
number of fixture calls in the test binary — the usual critique of
memory leaks (growing under long-running workloads) does not apply to
short-lived test processes.

## What not to do

- **Do not** reintroduce process-wide `static` record stores or
  catalogs in any form. The anti-pattern is captured by the guard
  test `nexus-server/tests/no_oncelock_globals.rs`.
- **Do not** root test fixtures at relative paths like
  `./data/test-foo`; every such path is a landmine under parallel
  test runs. Always go through `tempfile::tempdir()` or
  `TestContext::new()`.
- **Do not** assume `Executor::default()` shares state with any other
  `Executor::default()` caller. If you need two executors to cooperate,
  construct them both from the same explicit `Engine` / `Catalog` /
  `RecordStore` references.

## See also

- `phase3_remove-test-shared-state` task (archived) — the cleanup that
  eliminated `SHARED_STORE` and `SHARED_CATALOG`.
- `phase2a`–`phase2e` tasks (archived) — moved every API-module state
  off process-wide OnceLocks onto `NexusServer`.
- `nexus-server/tests/no_oncelock_globals.rs` — anti-regression guard
  that grep-fails on `static <NAME>: OnceLock<…>` in
  `nexus-server/src/api/`.
