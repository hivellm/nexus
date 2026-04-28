# 02 — Architecture Assessment

## Layered overview

```
HTTP/JSON (15474) · Binary RPC (15475) · RESP3
        │
        ▼
Cypher executor — parser → heuristic planner → physical operators
        │
        ▼
Transaction layer — epoch-based MVCC, single-writer per partition
        │
        ▼
Index layer — Label bitmap (Roaring) · B-tree · FTS (Tantivy 0.22) · KNN (HNSW) · R-tree (partial)
        │
        ▼
Storage — fixed records (32B node / 48B rel) · 8 KiB page cache · WAL · LMDB catalog
```

## 1. Storage layer (solid)

**Catalog** — LMDB via `heed` (10 GB max, 8 sub-DBs). Bidirectional label / type / property-key ↔ id maps, statistics (counts, NDV), schema metadata (epoch, version, page size). 21 unit tests, **98.64 %** coverage.

**Record stores** — `nodes.store` (32 B fixed: `label_bits: u64 | first_rel_ptr: u64 | prop_ptr: u64 | flags: u64`), `rels.store` (48 B fixed: src/dst/type/two-way next-ptrs/prop_ptr/flags). memmap2-backed with 1 MB → 2× exponential growth. Doubly-linked adjacency lists give **O(1) expand**. 96.96 % coverage.

**Page cache** — 8 KiB pages, three eviction policies: Clock (default), 2Q, TinyLFU. xxHash3 per-page integrity. Pin/unpin with atomic refcount. Hit-rate counters exposed via `/stats`. 96.15 % coverage.

**Strengths** — direct offset = `node_id × 32` for O(1) point reads. LMDB catalog is battle-tested. Page cache hot-path was hand-tuned (Clock cheaper than LRU under load).

**Limits**
- No prefetching — sequential scans incur random-I/O cost on cold pages.
- No column store / no batched property fetch — properties pulled one-record-at-a-time.
- Page-cache eviction for **property indexes** is `// TODO` in `cache/mod.rs` (`Check if index is actually cached` — uncapped memory under high-cardinality property indexes).
- Single max page-cache size; not adaptive to working set.

## 2. WAL + MVCC (solid)

**WAL** — 10 entry types (`BeginTx`, `CommitTx`, `CreateNode`, `SetProperty`, `Checkpoint`, plus FTS / R-tree variants). CRC32 per-entry. Format: `[epoch:8][tx_id:8][type:1][len:4][payload:N][crc32:4]`. fsync on commit. Checkpoint markers truncate the log. Replay is proven by 2310 passing tests including crash-recovery scenarios. 96.71 % coverage.

**MVCC** — global `u64` epoch counter, append-only versioning, visibility rule `created_epoch ≤ tx_epoch < deleted_epoch`. Readers pin a snapshot epoch — never block. Writers serialize on a `parking_lot::Mutex` per partition. 99.02 % coverage.

**Limit — single-writer** — per-partition serialization is the dominant write-throughput ceiling. Phase 9 reports baseline 162 qps → 604 qps after async/admission tuning, but the [`PERFORMANCE_ANALYSIS.md`](../../performance/PERFORMANCE_ANALYSIS.md) audit traced **all queries serialising on the executor lock** (Nexus 12 % CPU under load vs Neo4j 80 %+). The Phase-9 fix (`tokio::task::spawn_blocking` + `parking_lot::RwLock`) recovered some of the loss but no post-fix vs-Neo4j run is in the repo. **This is the biggest single perf-correctness scar.**

## 3. Index layer (mostly solid; spatial in flight)

| Index | Tech | Status | File |
|-------|------|--------|------|
| Label bitmap | `roaring::RoaringBitmap` | shipped, drives planner cardinality | `index/label_bitmap.rs` |
| B-tree (single + composite) | bespoke | shipped + backfill validator | `index/btree.rs`, `composite_btree.rs` |
| Full-text | Tantivy 0.22 | shipped + WAL integration + 9 analyzers + async writer | `index/fulltext*.rs` |
| KNN HNSW | `hnsw_rs` | shipped per-label, cosine/L2/dot, bytes-native | `index/knn.rs` |
| R-tree spatial | bespoke | **partial** — registry + bbox/withinDistance/nearest seek shapes wired in v1.2.0; full geospatial predicate execution still pending | `index/rtree/`, `executor/planner/queries.rs` |
| Constraints | UNIQUE / NODE KEY / NOT NULL / type-check | shipped, enforced on every write path | `index/constraints/` |

**Strength** — auto-populate on CREATE/SET/REMOVE/DELETE follows a uniform contract (FTS path mirrored by R-tree in v1.2.0). Membership tracking per-index avoids redundant work.

**Gap** — no `USING INDEX` / `USING SCAN` hints honored by planner (parsed but ignored). No partial / filtered / covering indexes. No histograms, only cardinality + NDV. Index selection is greedy (label bitmap preferred even where a property predicate is more selective).

## 4. Cypher executor + planner (heuristic; the long-term weak point)

**Coverage** — MATCH / OPTIONAL MATCH / WHERE / WITH / UNWIND / RETURN / ORDER BY / LIMIT / SKIP / UNION / FOREACH / CASE / EXISTS / pattern + list + map comprehensions / CALL subqueries / named paths / `shortestPath` / CREATE / MERGE / SET / DELETE / DETACH DELETE / REMOVE / SAVEPOINT / `GRAPH[<name>]` preamble / Cypher 25 `FOR ... REQUIRE` DDL. 250+ functions, ~100 APOC procedures, 19 GDS procedures.

**Operators** — `NodeByLabel`, `Filter` (vectorised where SIMD applies), `Expand` (linked-list pointer chase), `Project`, `OrderBy + Limit` (top-K heap), `Aggregate` (hash), `SpatialSeek` (Bbox / WithinDistance / Nearest, v1.2.0).

**Planner (`executor/planner/`)** — heuristic cost-based:
1. Filter pushdown.
2. Pattern reordering by selectivity.
3. Index selection (label > property > KNN, no contention model).
4. Limit pushdown (top-K heap).
5. Spatial seek rewrite (cost-compare against `NodeByLabel + Filter`, fall back to legacy when seek isn't cheaper). Bounded modes default 5 % selectivity; k-NN uses `k`.

**Where it's weak**
- No cardinality propagation — operators don't carry rolling estimates; cost is computed once at plan time.
- No join ordering algorithm (no DP, no branch-and-bound, no IDP1/IDP2). Multi-hop join order is greedy.
- No prepared-statement / plan-cache path — every query re-parses + re-plans.
- JIT compiler (`execution/jit/`) is **disabled**; ~80 % of the codegen module is `// TODO` Cranelift stubs. All execution is interpreted.
- Columnar fast-path real-world ratio is **~1.13×** (591 ms row vs 523 ms columnar @ 100 K rows); the SIMD compare/reduce kernels are saturated, materialisation cost dominates. SIMD micros are 4–16×; end-to-end is 1.1–2× — **honesty-as-feature**, but a pointer to where the next 2× could live.

## 5. Transaction model (correct, throughput-capped)

| Property | Nexus v1.13 | Neo4j 5.x | CockroachDB |
|---|---|---|---|
| Isolation | Snapshot (epoch MVCC) | Read-committed / repeatable-read | Serializable |
| Lock model | Single-writer per partition (`parking_lot::Mutex`) | MVCC + intent locks | Distributed MVCC + Raft |
| Single-node write throughput | 10–50 K ops/s (claimed) | 50–100 K ops/s | sharded-variable |
| Cross-partition writes | Eventual consistency | n/a (single node) | 2PC over Raft |
| Distributed transactions | **No** (V1 + V2 single-shard only) | n/a | Yes |

The simplification is sound for read-heavy RAG. It's a hard ceiling for write-heavy OLTP — a use case Nexus explicitly disclaims in `README.md`.

## 6. Distributed (V2) — core complete, ACID gap

**Sharding (`crates/nexus-core/src/sharding/`)** — xxh3 hash on node_id. Relationships live on the source node's shard with optional remote-anchor records. Iterative rebalance plan, generation-tagged metadata.

**Raft per shard (`sharding/raft/`)** — bespoke (not openraft). 65 unit tests, 3-node failover + 5-node partition + single-node bootstrap covered. §5.4.2 commit-from-current-term enforced. Wire format `[shard_id:u32][msg_type:u8][len:u32][payload][crc32:u32]`. Snapshot install reuses v1 zstd+tar replication snapshot.

**Coordinator (`coordinator/`)** — query classification (SingleShard / Targeted / Broadcast), plan decomposition with shard-local subplans + coordinator merge, scatter-gather with **atomic per-query failure** (any shard down → whole query fails; no partial rows), leader-hint retry (3×), `ERR_STALE_GEN` refresh (1 pass), LRU cross-shard cache (10 K entries, TTL).

**Cluster API (`/cluster/*`)** — status, add_node, remove_node, rebalance, shards/{id}. Admin-gated via existing RBAC.

**Real gap — cross-shard mutations.** V2 is **single-shard write only**. There's no 2PC across shards; multi-shard mutations are not supported. Coordinator failure semantics are atomic-fail-safe but there's no read-consistency contract across shards mid-query. This is the gating issue for V2 being "production cluster mode" vs "production single-node + read-fanout."

## 7. Auth / security (production-grade)

- **API keys**: 32-char random, Argon2 hashed in catalog, per-key permissions (read/write/admin/super), expiry.
- **JWT**: configurable expiry, session management.
- **RBAC**: User → Roles → Permissions enforced per endpoint.
- **Rate limiting**: 1 K/min, 10 K/hour per key, returns 429 + `X-RateLimit-*` headers.
- **3-layer back-pressure**: per-key limiter → per-connection RPC semaphore → global `AdmissionQueue` (FIFO wait + `503 Retry-After` on timeout). Light-weight diagnostic endpoints bypass the queue.
- **Audit log**: fail-open with `nexus_audit_log_failures_total` metric — IO pressure never converts to 500s.
- **TLS 1.3** via Tower stack; **mTLS** for service-to-service in V2.
- **Default**: auth disabled on `127.0.0.1`, required for `0.0.0.0`.

**Gaps** — no row-level / column-level security (Neo4j has fine-grained schema-based ACL). No encryption-at-rest provided by the engine (operator concern, OS/disk-level only). No SSO / OIDC / SAML adapters out of the box.

## 8. Engineering risk register

| # | Issue | Severity | Owner-recommendation |
|---|---|---|---|
| 1 | JIT codegen disabled (`execution/jit/` is mostly TODO) | High | finish or replace with bytecode VM (`~2–3 weeks`) |
| 2 | Global executor lock — needs post-Phase-9 re-bench vs Neo4j | High | re-run 74-test, publish v1.2 perf doc (`~1 week`) |
| 3 | No cross-shard 2PC | High | implement 2PC or pessimistic per-shard locking (`~4–6 weeks`) |
| 4 | R-tree only seek-shape; full spatial predicates pending | Medium | finish `phase6_opencypher-geospatial-predicates` (`~3–4 weeks`) |
| 5 | Quantified paths grammar-only | Medium | implement `QuantifiedExpand` operator (`~2–3 weeks`) |
| 6 | `CALL ... IN TRANSACTIONS` grammar-only | Medium | finish executor batching (`~1 week`) |
| 7 | Page-cache property-index eviction TODO | Medium | implement LRU/TTL on `cache/mod.rs` (`~3–5 days`) |
| 8 | Test isolation — 2 ignored tests using default data dir | Low | switch to `tempfile::tempdir()` (`~1 day`) |
| 9 | Planner has no cardinality propagation / join DP | Medium-long | research-grade; phased over `~4–8 weeks` |
| 10 | Version drift README/CHANGELOG/SDK | Low | release-train clean-up (`~1 day`) |

## 9. What's distinctive

- **Binary-RPC default** with bytes-native KNN embeddings is differentiated vs every Cypher peer (Neo4j Bolt is also binary, but FalkorDB / Memgraph default to RESP / Bolt; Nexus's MessagePack frame is purpose-built and 3–10× lower latency than HTTP/JSON in the same stack).
- **Three-layer back-pressure** with admission-queue + audit fail-open metric is more thoughtful than most graph DBs ship out of the box.
- **SIMD honesty doc** (`PERFORMANCE_V1.md`) — explicitly logs that CRC32C HW is *slower* than `crc32fast` on Zen 4, that simd-json *loses* to serde_json on `Value` targets, that columnar end-to-end is only 1.13× even though the kernels are 4–16×. Almost no other graph DB documents negative findings this clearly. Use this as a marketing trust-signal.

See [03_performance_results.md](03_performance_results.md) for the numbers, [09_distributed_v2_status.md](09_distributed_v2_status.md) for the cluster-mode deep dive, and [10_improvement_roadmap.md](10_improvement_roadmap.md) for the prioritised work list.
