# Implement MVP Storage Layer

## Why

Nexus currently has complete architecture documentation but no implementation. The MVP storage layer is the foundation for the entire graph database, providing:
- Durable storage via LMDB catalog and record stores
- ACID transactions via WAL and MVCC
- Performance via page cache
- Foundation for query execution and indexes

This is the first implementation phase that enables basic graph operations.

## What Changes

- Implement catalog module (LMDB integration for label/type/key mappings)
- Implement record stores (nodes.store, rels.store, props.store, strings.store)
- Implement page cache with Clock eviction policy
- Implement WAL (write-ahead log) with checkpoint and recovery
- Implement basic MVCC transaction manager (epoch-based snapshots)
- Add comprehensive tests (95%+ coverage requirement)

**BREAKING**: None (this is new implementation)

## Impact

### Affected Specs
- NEW capability: `storage-layer`
- NEW capability: `catalog`
- NEW capability: `transaction-manager`

### Affected Code
- `nexus-core/src/catalog/mod.rs` - LMDB integration (~300 lines)
- `nexus-core/src/storage/mod.rs` - Record stores (~800 lines)
- `nexus-core/src/page_cache/mod.rs` - Page management (~400 lines)
- `nexus-core/src/wal/mod.rs` - WAL implementation (~500 lines)
- `nexus-core/src/transaction/mod.rs` - MVCC manager (~300 lines)
- `tests/storage_tests.rs` - Integration tests (~600 lines)

### Timeline
- **Duration**: 2-3 weeks
- **Complexity**: High (foundational layer, requires careful design)
- **Dependencies**: None (Layer 0/1 in DAG)

