# Implement MVP Indexes (Label Bitmap & KNN)

## Why

After storage layer is complete, we need indexes to enable efficient queries. The MVP requires:
- Label bitmap index for fast node scanning by label
- KNN vector index for semantic similarity search
- Statistics for query planner cost estimation

These indexes are critical for query performance and enable the core hybrid search capability.

## What Changes

- Implement label bitmap index using RoaringBitmap
- Implement KNN vector index using hnsw_rs (HNSW algorithm)
- Add index statistics (cardinality, NDV)
- Integrate indexes with storage layer
- Add comprehensive tests (95%+ coverage)

**BREAKING**: None (new functionality)

## Impact

### Affected Specs
- NEW capability: `label-index`
- NEW capability: `knn-index`

### Affected Code
- `nexus-core/src/index/label.rs` - Label bitmap (~200 lines)
- `nexus-core/src/index/knn.rs` - HNSW integration (~400 lines)
- `nexus-core/src/index/stats.rs` - Statistics (~150 lines)
- `tests/index_tests.rs` - Integration tests (~300 lines)

### Dependencies
- Requires: `implement-mvp-storage` (must be complete first)

### Timeline
- **Duration**: 1 week
- **Complexity**: Medium

