## 1. Implementation
- [x] 1.1 Confirm with `grep -R "NEXUS_USE_MMAP_CATALOG"` that LMDB path is never entered in production or tests — actually verified the opposite direction: both branches of the env var gate in `Catalog::with_mmap` fell through to `Self::with_map_size`, which is the *LMDB* path. `MmapCatalog` was imported but never instantiated.
- [x] 1.2 Pick canonical backend (LMDB) and leave no trait indirection since the second backend was dead code, not an alternative implementation.
- [x] 1.3 Delete the dead `nexus-core/src/catalog/mmap_catalog.rs` file and the unreachable `with_mmap` wrapper.
- [x] 1.4 There were no duplicate caches to collapse — `MmapCatalog` allocated its own DashMaps but those allocations never ran. Removing the module removes the duplication.
- [x] 1.5 Remove the `NEXUS_USE_MMAP_CATALOG` env var gate; `Catalog::new` now does a single `with_map_size` call with the is-test aware map-size pick.

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation: `nexus-core/src/catalog/mod.rs` header comment records the phase2 dedup landing and the "implement behind a fresh trait rather than resurrecting the dead module" rule for any future mmap backend.
- [x] 2.2 Write tests covering the new behavior: the existing `catalog::tests::*` suite (31 tests) exercises add/lookup/delete/reopen round-trips and continues to pass against the simplified constructor. No new tests required because the deleted branch was never reachable — its removal is a cleanup, not a behaviour change.
- [x] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-core -p nexus-server`.
