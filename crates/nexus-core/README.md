# nexus-core

Core graph database engine for Nexus. Property graph + Cypher subset
+ native vector search. Embeddable as a library (`use nexus_core::*`)
or driven over the wire by [`nexus-server`](../nexus-server).

## What this crate is

The whole engine, minus the network layer. Every byte of storage,
every Cypher operator, every index, every transaction — all lives
here. `nexus-server` and `nexus-cli` are thin shells around it;
`nexus-bench` deliberately does **not** depend on it.

## Layered architecture

```
┌─────────────────────────────────────────────────────────┐
│                  Cypher Executor                        │
│   parser · planner · operators (match, expand, filter,  │
│   project, aggregate, sort, optional-match, subquery)   │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────┴──────────────────────────────┐
│              Transaction Layer (MVCC)                   │
│   epoch-based snapshots · single-writer locking         │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────┴──────────────────────────────┐
│                    Index Layer                          │
│  label bitmap (RoaringBitmap) · B-tree · full-text      │
│  (Tantivy) · KNN (HNSW) · constraint indexes            │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────┴──────────────────────────────┐
│                   Storage Layer                         │
│  catalog (LMDB / heed) · record stores (memmap2) ·      │
│  page cache (clock / 2Q / TinyLFU) · WAL · property     │
│  chains · zstd-compressed snapshots                     │
└─────────────────────────────────────────────────────────┘
```

## Module map

| Path | Purpose |
|---|---|
| `catalog/` | Label / type / property-key ↔ ID bidirectional maps (LMDB) |
| `storage/` | Fixed-size record stores: nodes (32 B), rels (48 B), props |
| `page_cache/` | 8 KB pages with pluggable eviction (clock / 2Q / TinyLFU) |
| `wal/` | Append-only write-ahead log + crash recovery |
| `transaction/` | Epoch-based MVCC, single-writer locking, isolation |
| `index/` | Label bitmap, B-tree V1, full-text (Tantivy), KNN (hnsw_rs) |
| `executor/` | Cypher parser, heuristic cost-based planner, operators |
| `engine/` | Top-level `Engine` facade tying storage + indexes + executor |
| `database/` | `DatabaseManager` — multi-database isolation per server |
| `auth/` | RBAC, API keys (Argon2), JWT (rust_crypto backend), audit log |
| `cluster/` · `coordinator/` · `sharding/` · `replication/` | V2 distributed primitives + Raft consensus |
| `graph/` | Graph algorithms, correlation analysis, vectorizer extraction |
| `apoc/` | APOC-compatible procedures (`apoc.text.*`, `apoc.coll.*`, …) |
| `simd/` | SSE4.2 / AVX2 hot paths: distance, popcount, CRC, reduce, RLE |
| `geospatial/` · `spatial/` | Point / WKT / spatial-index support |
| `udf/` | User-defined function registry |
| `plugin/` | Plugin host for extension points |
| `monitoring.rs` · `query_cache.rs` · `vectorizer_cache.rs` | Observability + caches |
| `testing/` | Test fixtures (gated behind the `testing` feature) |

## Cargo features

| Feature | Default | Effect |
|---|---|---|
| `s2s` | off | Server-to-server cluster transport types |
| `slow-tests` | off | Opt in to long-running integration tests |
| `benchmarks` | off | Expose internals needed by Criterion harnesses |
| `testing` | off | Re-export test helpers for downstream crates |
| `axum` | off | Compile in middleware that depends on `axum::extract::*` |

## Build & test

```bash
cargo +nightly build --release -p nexus-core
cargo +nightly test  --workspace -p nexus-core
cargo +nightly test  --workspace -p nexus-core --features slow-tests

cargo +nightly clippy -p nexus-core --all-targets --all-features -- -D warnings
cargo +nightly fmt --all
```

The workspace currently runs at **2310 passed / 67 ignored / 0 failed**
on `cargo +nightly test --workspace`; `nexus-core` owns the bulk of
that surface.

## Benchmarks

Criterion harnesses live under `benches/` and are wired through
`Cargo.toml` `[[bench]]` entries:

| Bench | Measures |
|---|---|
| `protocol_point_read` | RPC point-read round-trip vs. raw store read |
| `executor_filter` · `executor_aggregate` | Operator hot paths |
| `parser_tokenize` | Cypher tokenizer throughput |
| `simd_distance` · `simd_popcount` · `simd_reduce` · `simd_compare` · `simd_crc` · `simd_json` · `simd_rle` | Per-kernel SIMD speedups |
| `optional_match_benchmark` · `exists_subquery_benchmark` | Phase-6 operators |
| `cluster_mode_benchmark` · `v2_tcp_transport` | V2 distributed primitives |
| `graph_correlation_benchmark` · `vectorizer_extraction_benchmark` | Graph analytics |
| `fulltext_bench` | Tantivy full-text search |

```bash
cargo +nightly bench -p nexus-core --bench executor_filter
```

## Examples

Two end-to-end examples live in the repo-root `examples/` directory
and are wired through `[[example]]` entries in this crate:

```bash
cargo +nightly run --release -p nexus-core --example hierarchical_call_graph_example
cargo +nightly run --release -p nexus-core --example call_graph_filtering_example
```

## Hard constraints

- **Neo4j wire-format compatibility.** `rows` are always Neo4j-style
  arrays (`[[v1, v2]]`), never object maps. SDKs add `RowsAsMap()`
  helpers — do not change the server format. (310/325 on the
  Neo4j 2025.09.0 diff suite.)
- **No `unwrap()` in non-test code** outside of obvious invariants.
  Use `?` + `thiserror` (this crate is a library).
- **No `unsafe` without a `// SAFETY:` comment.**
- **Single-writer transactions.** One writer per partition; readers
  use epoch-based MVCC snapshots and never block.
- **Database isolation.** Each database has its own catalog, stores,
  WAL, and indexes. No cross-database queries by design.
- **95 % test coverage minimum** for new code
  (`cargo llvm-cov --workspace --ignore-filename-regex 'examples'`).

## Embedding

```rust
use nexus_core::Engine;

let engine = Engine::open("./data/mygraph")?;
let result = engine
    .executor()
    .execute("MATCH (n:Person) RETURN n.name LIMIT 10", Default::default())
    .await?;

for row in result.rows {
    println!("{:?}", row);
}
```

For network access, use [`nexus-server`](../nexus-server) (HTTP /
MCP / GraphQL) or [`nexus-protocol`](../nexus-protocol) (binary RPC,
REST, MCP, UMICP clients).

## Links

- Architecture: [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md)
- Storage spec: [`docs/specs/storage-format.md`](../../docs/specs/storage-format.md)
- Cypher subset: [`docs/specs/cypher-subset.md`](../../docs/specs/cypher-subset.md)
- WAL / MVCC: [`docs/specs/wal-mvcc.md`](../../docs/specs/wal-mvcc.md)
- KNN integration: [`docs/specs/knn-integration.md`](../../docs/specs/knn-integration.md)
- Neo4j compat report: [`docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`](../../docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md)
