# Changelog

All notable changes to Nexus will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### In Progress (MVP - Phase 1)

- Storage layer implementation
- Cypher executor development
- HTTP API endpoints
- KNN integration

## [0.1.0] - 2024-10-24

### Added

- **Project Initialization**
  - Cargo workspace setup (edition 2024, nightly)
  - Module structure (nexus-core, nexus-server, nexus-protocol)
  - Comprehensive architecture documentation

- **Documentation**
  - [ARCHITECTURE.md](docs/ARCHITECTURE.md) - Complete system design
  - [ROADMAP.md](docs/ROADMAP.md) - Implementation phases and timeline
  - [DAG.md](docs/DAG.md) - Component dependency graph
  - [storage-format.md](docs/specs/storage-format.md) - Record store layouts
  - [cypher-subset.md](docs/specs/cypher-subset.md) - Supported Cypher syntax
  - [page-cache.md](docs/specs/page-cache.md) - Memory management design
  - [wal-mvcc.md](docs/specs/wal-mvcc.md) - Transaction model
  - [knn-integration.md](docs/specs/knn-integration.md) - Vector search integration
  - [api-protocols.md](docs/specs/api-protocols.md) - REST, MCP, UMICP specs
  - README.md - Project overview and quick start
  - CHANGELOG.md - This file

- **Core Module Scaffolding** (nexus-core)
  - `error` - Error types and Result aliases
  - `catalog` - Label/Type/Key ID mappings (LMDB)
  - `storage` - Record stores (nodes, rels, props, strings)
  - `page_cache` - Page management with eviction policies
  - `wal` - Write-ahead log for durability
  - `index` - Indexing subsystems (label bitmap, B-tree, full-text, KNN)
  - `executor` - Cypher query executor (parser, planner, operators)
  - `transaction` - MVCC and locking

- **Server Scaffolding** (nexus-server)
  - Axum HTTP server setup
  - REST API endpoints (stubs):
    - `GET /health` - Health check
    - `POST /cypher` - Execute Cypher queries
    - `POST /knn_traverse` - KNN-seeded traversal
    - `POST /ingest` - Bulk data ingestion
  - Configuration management

- **Protocol Layer** (nexus-protocol)
  - REST client for external integrations
  - MCP client stub
  - UMICP client stub

- **Build Infrastructure**
  - `.gitignore` for Rust projects
  - `rust-toolchain.toml` (nightly, edition 2024)
  - Workspace dependencies in `Cargo.toml`
  - LICENSE (MIT OR Apache-2.0)

### Dependencies

- **Storage**: memmap2, heed (LMDB), parking_lot, roaring
- **Indexes**: tantivy, hnsw_rs
- **Async**: tokio, axum, tower, hyper
- **Serialization**: serde, serde_json, bincode, bytes, bytemuck
- **Error**: thiserror, anyhow
- **Observability**: tracing, tracing-subscriber
- **Utilities**: uuid, chrono

### Testing

- Test structure defined in `/tests` directory
- 95% coverage requirement documented
- Integration test framework prepared

## [0.0.0] - 2024-10-23

### Initial Concept

- Project planning and architecture design
- Technology stack selection (Rust, LMDB, HNSW, Axum)
- Neo4j-inspired storage model research

---

## Versioning Strategy

- **MAJOR** (x.0.0): Breaking API changes, storage format changes
- **MINOR** (0.x.0): New features, backwards compatible
- **PATCH** (0.0.x): Bug fixes, performance improvements

---

## Upcoming Releases

### [0.2.0] - MVP Core (Planned: Q4 2024)

#### Storage Layer

- [x] Catalog implementation (LMDB)
- [ ] Record stores (nodes, rels, props, strings)
- [ ] Page cache with clock eviction
- [ ] WAL with checkpoint/recovery
- [ ] MVCC transaction manager

#### Indexes

- [ ] Label bitmap index (RoaringBitmap)
- [ ] KNN vector index (HNSW)
- [ ] Index statistics for query planner

#### Query Execution

- [ ] Cypher parser (basic patterns)
- [ ] Query planner (heuristic cost-based)
- [ ] Physical operators:
  - [ ] NodeByLabel
  - [ ] Filter
  - [ ] Expand
  - [ ] Project
  - [ ] OrderBy + Limit
  - [ ] Aggregate (COUNT, SUM, AVG, MIN, MAX)

#### API

- [ ] Complete REST endpoints
- [ ] Error handling and validation
- [ ] Query timeout support
- [ ] Bulk ingestion

#### Testing

- [ ] Unit tests (95%+ coverage)
- [ ] Integration tests
- [ ] Performance benchmarks
- [ ] Crash recovery tests

### [0.3.0] - V1 Advanced Features (Planned: Q1 2025)

- [ ] Property B-tree indexes
- [ ] Full-text search (Tantivy)
- [ ] Constraints (UNIQUE, NOT NULL)
- [ ] Query optimization (cost model)
- [ ] Bulk loader (bypass WAL)
- [ ] Prometheus metrics
- [ ] OpenAPI specification

### [0.4.0] - V2 Distributed (Planned: Q2 2025)

- [ ] Sharding architecture
- [ ] Raft consensus (openraft)
- [ ] Read replicas
- [ ] Distributed query coordinator
- [ ] Cluster management

---

## Notes

### Breaking Changes Policy

- Breaking changes only in major version bumps
- Deprecation warnings 2 minor versions before removal
- Migration guides provided for all breaking changes

### Security Updates

- Security patches released as PATCH versions
- Security advisories published on GitHub
- CVE tracking for production releases

### Performance Targets

Maintained across versions:

- Point reads: 100K+ ops/sec
- KNN queries: 10K+ ops/sec
- Pattern traversal: 1K-10K ops/sec
- 95%+ test coverage
- Zero known critical bugs

---

## Links

- **Repository**: https://github.com/hivellm/nexus
- **Documentation**: https://docs.nexus-db.io
- **Releases**: https://github.com/hivellm/nexus/releases
- **Issues**: https://github.com/hivellm/nexus/issues

