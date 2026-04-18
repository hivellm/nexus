# Proposal: phase2_deduplicate-catalog-backends

## Why

`nexus-core/src/catalog/` has two catalog implementations living in parallel:
`mod.rs` (LMDB-backed, with six `DashMap` caches for label/type/key lookups)
and `mmap_catalog.rs` (memory-map-backed, with the same six `DashMap` caches).
The `Catalog::new` constructor actually routes everything through the mmap
backend today (`NEXUS_USE_MMAP_CATALOG=true` default), leaving the LMDB
scaffolding as half-dead code. The duplicated cache structures also double
the schema-level memory footprint for nothing.

## What Changes

- Pick one backend as canonical. The running system already prefers mmap,
  so the obvious choice is to make `mmap_catalog` the only implementation
  and keep `Catalog` as a thin public façade.
- Collapse the duplicated `DashMap<String, u32>` caches into a single set
  owned by the façade.
- Delete LMDB-only code paths that are no longer reachable.
- Update docs and any env-var mentions that still reference
  `NEXUS_USE_MMAP_CATALOG`.

## Impact

- Affected specs: `docs/specs/storage-format.md` (catalog section)
- Affected code:
  - `nexus-core/src/catalog/mod.rs:122-132` (duplicate DashMap fields)
  - `nexus-core/src/catalog/mmap_catalog.rs:79-84` (same DashMaps)
  - any caller that touches `Catalog` internals directly
- Breaking change: NO for the public `Catalog` API; internally a lot
  disappears
- User benefit: one code path to reason about; lower resident memory for
  catalog caches; removes a half-migrated architecture that confused the
  `fix/memory-leak-v1` debugging
