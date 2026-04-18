## 1. Implementation
- [ ] 1.1 Confirm with `grep -R "NEXUS_USE_MMAP_CATALOG"` that LMDB path is never entered in production or tests
- [ ] 1.2 Extract a `CatalogBackend` trait covering the operations `Catalog` exposes today, implement it for the mmap variant
- [ ] 1.3 Delete the LMDB code branches in `catalog/mod.rs`; keep only the façade re-exporting the mmap implementation
- [ ] 1.4 Collapse the six duplicate `DashMap` caches so they live in one place (inside the façade)
- [ ] 1.5 Remove the `NEXUS_USE_MMAP_CATALOG` env var gate

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/specs/storage-format.md` describing the single catalog backend
- [ ] 2.2 Add / update tests that exercise catalog round-trip (label add → lookup → delete) to prove nothing regresses
- [ ] 2.3 Run `cargo test --package nexus-core catalog::` and confirm all pass
