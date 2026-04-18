## 1. Implementation
- [ ] 1.1 Declare `pub const DEFAULT_VECTORIZER_DIMENSION: usize = 128;` in `nexus-core/src/index/mod.rs` with a docstring explaining the choice
- [ ] 1.2 Replace `KnnIndex::new_default(128)` and `KnnIndex::new(128)` with `KnnIndex::new_default(DEFAULT_VECTORIZER_DIMENSION)` everywhere (49 sites)
- [ ] 1.3 Declare `pub const CATALOG_MMAP_INITIAL_SIZE: usize = 100 << 20;` in `nexus-core/src/catalog/mod.rs`; replace the 22 literal occurrences
- [ ] 1.4 Consolidate the page-size literal into a single `pub const PAGE_SIZE_BYTES: usize = 8 * 1024;`
- [ ] 1.5 Remove the `unused manifest key: example.1.num_cpus` warning in root Cargo.toml while we're here

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/performance/MEMORY_TUNING.md` cross-referencing the new constants
- [ ] 2.2 Existing tests already exercise these defaults — no new tests required, just verify nothing regresses
- [ ] 2.3 Run `cargo test --workspace` and confirm pass
