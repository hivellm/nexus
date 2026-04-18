# Proposal: phase4_extract-magic-constants

## Why

A few numeric literals are copy-pasted across the codebase and every copy
has to be maintained in sync by hand:

- `KnnIndex::new_default(128)` appears in 49 places across `executor`,
  tests, examples, and integration benches. Nothing documents why 128 is
  the default vectorizer dimension.
- `100 * 1024 * 1024` (the catalog mmap file size) is repeated 22 times
  across `catalog/mod.rs` and `mmap_catalog.rs`.
- `8 * 1024` (page size) shows up inline in storage, page cache, and
  tests.

The `fix/memory-leak-v1` branch already added `INITIAL_NODE_CAPACITY`,
`INITIAL_REL_CAPACITY`, `MAX_INTERMEDIATE_ROWS`, etc. This task finishes
the job for the constants that are still inline.

## What Changes

- Extract `pub const DEFAULT_VECTORIZER_DIMENSION: usize = 128;` in
  `nexus-core/src/index/mod.rs` and replace the 49 literal occurrences.
- Extract `pub const CATALOG_MMAP_INITIAL_SIZE: usize = 100 << 20;` and
  replace the 22 occurrences.
- Extract `pub const PAGE_SIZE_BYTES: usize = 8 * 1024;` (may already
  exist — consolidate if so).
- Add module-level `//!` comments explaining *why* those are the defaults.

## Impact

- Affected specs: none
- Affected code: `nexus-core/src/index/mod.rs`, `nexus-core/src/catalog/`,
  `nexus-core/src/page_cache/`, example + test files that interpolate 128
- Breaking change: NO (pure rename)
- User benefit: one place to tune each knob; the cost of the choice is
  documented
