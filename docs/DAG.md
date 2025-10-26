# Component Dependency Graph (DAG)

This document defines the dependency relationships between Nexus components to guide implementation order and prevent circular dependencies.

## Visualization

```
                    ┌─────────────────┐
                    │  nexus-server   │
                    │  (HTTP/REST)    │
                    └────────┬────────┘
                             │
                   ┌─────────┴─────────┐
                   │                   │
          ┌────────▼────────┐  ┌──────▼──────────┐
          │  nexus-core     │  │ nexus-protocol  │
          │  (Engine)       │  │ (REST/MCP/...)  │
          └────────┬────────┘  └─────────────────┘
                   │
       ┌───────────┼───────────┬───────────┐
       │           │           │           │
  ┌────▼────┐ ┌───▼────┐ ┌────▼────┐ ┌───▼────┐
  │Executor │ │  Index │ │  Trans  │ │  WAL   │
  └────┬────┘ └───┬────┘ └────┬────┘ └───┬────┘
       │          │           │           │
       └──────────┼───────────┴───────────┘
                  │
         ┌────────┴────────┬──────────┐
         │                 │          │
    ┌────▼────┐      ┌────▼────┐ ┌───▼────┐
    │ Storage │      │  Cache  │ │Catalog │
    └────┬────┘      └────┬────┘ └───┬────┘
         │                │          │
         └────────────────┴──────────┘
                      │
                 ┌────▼────┐
                 │  Error  │
                 └─────────┘
```

## Layer Hierarchy (Bottom-Up)

### Layer 0: Foundation

**No dependencies**

- `error` - Error types and Result aliases
  - Provides: `Error`, `Result<T>`
  - Dependencies: `thiserror`, `std`

### Layer 1: Storage Primitives

**Depends on**: Layer 0

- `catalog` - Label/Type/Key mappings
  - Provides: Bidirectional ID mappings
  - Dependencies: `error`, `heed` (LMDB)
  - Used by: All higher layers needing ID resolution

- `page_cache` - Page management
  - Provides: Page allocation, eviction, pin/unpin
  - Dependencies: `error`, `memmap2`, `parking_lot`
  - Used by: `storage`, `index`

- `storage` - Record stores
  - Provides: Node/Rel/Prop record I/O
  - Dependencies: `error`, `page_cache`, `memmap2`
  - Used by: `executor`, `transaction`, `index`

### Layer 2: Durability & Indexing

**Depends on**: Layers 0-1

- `wal` - Write-Ahead Log
  - Provides: Transaction logging, recovery
  - Dependencies: `error`, `storage`, `serde`
  - Used by: `transaction`

- `index` - Indexing subsystems
  - Provides: Label bitmap, B-tree, full-text, KNN
  - Dependencies: `error`, `storage`, `page_cache`, `roaring`, `tantivy`, `hnsw_rs`
  - Used by: `executor`

### Layer 3: Transaction Management

**Depends on**: Layers 0-2

- `transaction` - MVCC and locking
  - Provides: Transaction begin/commit/abort, epoch management
  - Dependencies: `error`, `storage`, `wal`, `parking_lot`
  - Used by: `executor`

### Layer 4: Query Execution

**Depends on**: Layers 0-3

- `executor` - Cypher query execution
  - Provides: Query parsing, planning, execution
  - Dependencies: `error`, `storage`, `index`, `transaction`, `catalog`
  - Used by: `nexus-server`

### Layer 5: Engine Facade

**Depends on**: Layers 0-4

- `lib.rs` (Engine) - Public API facade
  - Provides: Unified `Engine` interface
  - Dependencies: All lower layers
  - Used by: `nexus-server`

### Layer 6: Network Layer

**Depends on**: Layers 0-5

- `nexus-protocol` - Integration protocols
  - Provides: REST/MCP/UMICP clients
  - Dependencies: `hyper`, `serde_json`, `tokio`
  - Used by: `nexus-server` (optional, for external integrations)

- `nexus-server` - HTTP API server
  - Provides: REST endpoints
  - Dependencies: `nexus-core`, `nexus-protocol`, `axum`, `tower`
  - Used by: External clients

## Implementation Order

### Phase 1: Foundation (Week 1-2)

1. `error` - Define all error types
2. `catalog` - LMDB integration, ID mappings
3. `page_cache` - Basic clock eviction
4. `storage` - Record stores (nodes, rels, props)

**Milestone**: Can create/read nodes and relationships

### Phase 2: Durability (Week 3-4)

5. `wal` - Append-only log, checkpoint
6. `transaction` - MVCC epochs, single-writer locks

**Milestone**: ACID transactions with crash recovery

### Phase 3: Indexing (Week 5)

7. `index/label` - RoaringBitmap per label
8. `index/knn` - HNSW integration

**Milestone**: Fast label scans and KNN queries

### Phase 4: Query (Week 6-8)

9. `executor/parser` - Basic Cypher parser
10. `executor/planner` - Heuristic cost-based planning
11. `executor/operators` - Physical operators (Scan, Filter, Expand, Project)

**Milestone**: Execute basic Cypher queries

### Phase 5: API (Week 9)

12. `nexus-server/api` - REST endpoints
13. `nexus-protocol` - External integrations

**Milestone**: Full HTTP API

## Dependency Rules

### Allowed Dependencies

- **Down the stack**: Higher layers can depend on lower layers
- **Sibling (same layer)**: Only if no circular dependency
- **External crates**: Must be workspace dependencies

### Forbidden Dependencies

- **Up the stack**: Lower layers CANNOT depend on higher layers
- **Circular**: No circular dependencies between modules
- **Direct I/O in high layers**: Only storage layer touches files

### Examples

✅ **Allowed**:
```rust
// executor depends on storage (higher → lower)
use crate::storage::RecordStore;

// transaction depends on wal (same layer, no cycle)
use crate::wal::Wal;

// index depends on storage (higher → lower)
use crate::storage::NodeRecord;
```

❌ **Forbidden**:
```rust
// storage depends on executor (lower → higher) ❌
use crate::executor::Executor;

// circular: A → B → A ❌
// module A
use crate::B;
// module B
use crate::A;

// executor doing direct file I/O (should use storage) ❌
use std::fs::File;
File::open("nodes.store")?;
```

## Module Ownership

### Primary Responsibilities

| Module | Owns | Does NOT Own |
|--------|------|--------------|
| `catalog` | Label/Type/Key IDs | Node/relationship data |
| `storage` | Record files, record I/O | Indexes, query planning |
| `page_cache` | Memory pages, eviction | What's stored in pages |
| `wal` | Transaction log | Transaction semantics |
| `transaction` | MVCC, locks | Query execution |
| `index` | Secondary indexes | Primary storage |
| `executor` | Query planning, execution | Direct storage access |

### Interaction Patterns

```rust
// Executor queries via clean interfaces:
fn execute_query() -> Result<ResultSet> {
    // Get node IDs from index
    let node_ids = self.index.get_nodes_by_label(label_id)?;
    
    // Read nodes from storage
    let nodes: Vec<NodeRecord> = node_ids
        .iter()
        .map(|id| self.storage.read_node(*id))
        .collect::<Result<_>>()?;
    
    // Filter using transaction snapshot
    let visible: Vec<_> = nodes
        .into_iter()
        .filter(|n| self.tx.is_visible(n))
        .collect();
    
    Ok(ResultSet::from(visible))
}
```

## Testing Boundaries

### Unit Tests (Per Module)

Each module tests its own functionality in isolation:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_catalog_bidirectional_mapping() {
        // Test only catalog logic, mock dependencies
    }
}
```

### Integration Tests (Cross-Module)

Test layer boundaries:

```rust
// tests/storage_integration.rs
#[test]
fn test_storage_with_page_cache() {
    let cache = PageCache::new(1000)?;
    let store = RecordStore::new(cache)?;
    // Test storage + cache interaction
}
```

### End-to-End Tests (Full Stack)

Test complete flows:

```rust
// tests/e2e_query.rs
#[test]
fn test_full_cypher_query() {
    let engine = Engine::new()?;
    let result = engine.execute("MATCH (n:Person) RETURN n")?;
    assert_eq!(result.rows.len(), 10);
}
```

## Dependency Injection

Use trait bounds for testability:

```rust
// Storage abstraction
pub trait Storage {
    fn read_node(&self, id: u64) -> Result<NodeRecord>;
    fn write_node(&mut self, id: u64, record: &NodeRecord) -> Result<()>;
}

// Executor depends on trait, not concrete type
pub struct Executor<S: Storage> {
    storage: S,
}

// In production: use real storage
let executor = Executor::new(RecordStore::new()?);

// In tests: use mock
let executor = Executor::new(MockStorage::new());
```

## Build Optimizations

### Parallel Compilation

Modules with no interdependencies can build in parallel:

```toml
# Cargo automatically parallelizes when dependencies allow
# Layer 0: error (builds first, alone)
# Layer 1: catalog, page_cache, storage (parallel)
# Layer 2: wal, index (parallel, after Layer 1)
# etc.
```

### Incremental Compilation

Changes to lower layers require rebuilding higher layers:

- Change `error` → rebuild everything
- Change `storage` → rebuild executor, transaction, index, server
- Change `executor` → rebuild only server
- Change `server` → rebuild only server

Minimize changes to foundation layers to maintain fast iteration.

## External Dependencies Graph

```
nexus-core:
  memmap2 ─→ [storage, page_cache]
  heed ────→ [catalog]
  roaring ─→ [index]
  tantivy ─→ [index] (V1)
  hnsw_rs ─→ [index]
  parking_lot → [transaction, page_cache]
  thiserror ─→ [error]
  serde ────→ [all modules]
  tokio ────→ [transaction (async locks)]

nexus-server:
  axum ────→ [main, api]
  tower ───→ [main]
  hyper ───→ [main]
  tokio ───→ [main, runtime]

nexus-protocol:
  hyper ───→ [rest]
  tokio ───→ [all]
```

## Version Compatibility

| Module | Rust Version | Edition | Stability |
|--------|--------------|---------|-----------|
| nexus-core | nightly 1.85+ | 2024 | Unstable (MVP) |
| nexus-server | nightly 1.85+ | 2024 | Unstable (MVP) |
| nexus-protocol | nightly 1.85+ | 2024 | Unstable (MVP) |

After MVP: stabilize on stable Rust if edition 2024 is available on stable.

## Future DAG Extensions

### V1 Additions

- `constraints` module (depends on `catalog`, `index`)
- `optimizer` module (depends on `executor`)
- `bulk_loader` module (depends on `storage`, bypasses `wal`)

### V2 Additions

- `replication` crate (depends on `nexus-core`, `openraft`)
- `coordinator` module in `nexus-server` (depends on `replication`)
- `sharding` module (depends on `storage`, `catalog`)

