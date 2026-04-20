# Nexus Graph Database

**⚡ High-performance property graph database with native vector search and binary-RPC-first clients**

![Rust](https://img.shields.io/badge/rust-nightly%201.85%2B-orange.svg)
![Edition](https://img.shields.io/badge/edition-2024-blue.svg)
![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)
![Status](https://img.shields.io/badge/status-v1.0.0-success.svg)
![Tests](https://img.shields.io/badge/tests-3361%20passing-success.svg)
![Compatibility](https://img.shields.io/badge/Neo4j%20compat-299%2F300-success.svg)

[Quick Start](#-quick-start) • [Transports](#-transports) • [Documentation](#-documentation) • [SDKs](#-official-sdks) • [Roadmap](#-roadmap) • [Contributing](#-contributing)

---

## 🎯 What is Nexus?

Nexus is a **property graph database** built for **read-heavy workloads** with **first-class vector search**. Inspired by Neo4j's storage architecture, it combines graph traversal with approximate nearest-neighbor search for hybrid **RAG**, **recommendation**, and **knowledge-graph** applications.

**Think Neo4j meets vector search**, shipped as a single Rust binary with a CLI, six first-party SDKs, and three transports (native binary RPC, HTTP/JSON, RESP3).

### Highlights (v1.0.0)

- **Neo4j-compatible Cypher** — ~55% openCypher coverage across MATCH / CREATE / MERGE / SET / DELETE / REMOVE / WHERE / RETURN / ORDER BY / LIMIT / SKIP / UNION / WITH / UNWIND / FOREACH and ~60 functions. **300/300** Neo4j compatibility tests pass ([diffed against Neo4j 2025.09.0 on 2026-04-19](docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md)).
- **Native KNN** — per-label HNSW indexes with cosine / L2 / dot metrics. Bytes-native embeddings on the RPC wire (no base64 tax).
- **Binary RPC default** — length-prefixed MessagePack on port `15475`; 3–10× lower latency and 40–60% smaller payloads vs HTTP/JSON. `nexus://host:15475` URL scheme used by CLI + every SDK.
- **Full auth stack** — API keys, JWT, RBAC, rate limiting, audit log with fail-open policy.
- **Multi-database** — isolated databases in a single server instance, CLI + Cypher + SDK.
- **SIMD-accelerated hot paths** — AVX-512 / AVX2 / NEON runtime dispatch. 12.7× KNN dot @ dim=768 on Zen 4, 7.9× `sum_f64` @ 262K rows, all with proptest-enforced parity vs scalar.
- **3361+ unit tests** across the workspace, 70%+ coverage.

## 🚀 Quick Start

### Install (pre-built binary)

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install/install.sh | bash

# Windows (PowerShell)
powershell -c "irm https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install/install.ps1 | iex"
```

The installer puts `nexus-server` and `nexus` (CLI) on `PATH` and registers a system service.

### Install (from source)

```bash
git clone https://github.com/hivellm/nexus
cd nexus
cargo +nightly build --release --workspace
./target/release/nexus-server        # default: 127.0.0.1:15474 (HTTP) + 127.0.0.1:15475 (RPC)
```

### First query with the CLI

```bash
./target/release/nexus query "RETURN 1 + 2 AS sum, 'hello' AS greeting"
# ┌─────┬──────────┐
# │ sum │ greeting │
# ╞═════╪══════════╡
# │ 3.0 │ hello    │
# └─────┴──────────┘
```

The CLI defaults to `nexus://127.0.0.1:15475` (binary RPC). Every subcommand (`query`, `db`, `user`, `key`, `schema`, `data`) goes through RPC. See [`crates/nexus-cli/README.md`](crates/nexus-cli/README.md) for the full command reference.

### First query from a SDK (Rust)

```rust
use nexus_sdk::NexusClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = NexusClient::new("nexus://127.0.0.1:15475")?;
    let result = client.execute_cypher("MATCH (n:Person) RETURN n.name LIMIT 10", None).await?;
    println!("{} rows via {}", result.rows.len(), client.endpoint_description());
    Ok(())
}
```

See [sdks/](sdks/README.md) for install + quick-start in every language.

### First query over HTTP (legacy / diagnostic)

```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{"query": "RETURN 1 + 2 AS sum"}'
# {"columns":["sum"],"rows":[[3]],"execution_time_ms":1}
```

## 🧬 Transports

| URL form                  | Transport              | Default port | Use case                                     |
|---------------------------|------------------------|--------------|----------------------------------------------|
| `nexus://host[:port]`     | Binary RPC (MessagePack) | `15475`    | **Default.** CLI + SDK. Fastest path.       |
| `http://host[:port]`      | HTTP/JSON              | `15474`      | Browser builds, firewalls, diagnostic.      |
| `https://host[:port]`     | HTTPS/JSON             | `443`        | Public-internet HTTP with TLS.              |
| `resp3://host[:port]`     | RESP3 (Redis-style)    | `15476`      | `redis-cli` / `iredis` / Grafana debug tail. |

Every CLI and SDK reads a URL scheme, honours the `NEXUS_SDK_TRANSPORT` / `NEXUS_TRANSPORT` env var, and falls back to an explicit `transport` field in config. The URL scheme wins over env/config. Full spec: [`docs/specs/sdk-transport.md`](docs/specs/sdk-transport.md).

**There is no `nexus-rpc://` or `nexus+rpc://` scheme.** The canonical single-token scheme is `nexus`.

## 🌟 Features

### Graph engine
- **Property graph model** — labels + types + properties on nodes and relationships.
- **Cypher subset** — familiar Neo4j syntax. See [docs/specs/cypher-subset.md](docs/specs/cypher-subset.md).
- **Record-store architecture** — 32-byte nodes, 48-byte relationships, O(1) traversal via linked adjacency lists.
- **ACID transactions** — single-writer + epoch-based MVCC, WAL for crash recovery.
- **19 GDS procedures** — PageRank (standard / weighted / parallel), betweenness, eigenvector, Dijkstra, A\*, Yen's k-paths, Louvain, label propagation, triangle count, clustering coefficients.

### Vector search (native)
- **HNSW indexes** per label (`cosine` / `l2` / `dot`).
- **Bytes-native embeddings** on the RPC wire — send `NexusValue::Bytes` of little-endian `f32` directly, no base64.
- **Hybrid queries** — combine `CALL vector.knn` with graph traversal in a single Cypher statement.

### Performance
- **Runtime SIMD dispatch** — `NEXUS_SIMD_DISABLE=1` for emergency rollback, no rebuild.
- **Linear-time Cypher parser** — O(N²) → O(N) fix, ~290× faster on 32 KiB queries.
- **Hierarchical cache** (L1/L2/L3) with 90%+ hit rates.
- **Runtime-dispatched CRC32C** for WAL (hardware-accelerated).
- **JIT query compilation** (Cranelift) for hot paths.

### Security + Ops
- **API keys + JWT + RBAC** with rate limiting (1k/min, 10k/hour default).
- **Audit logging** — fail-open with `nexus_audit_log_failures_total` metric; never converts IO pressure into mass 500s. Rationale: [`docs/security/SECURITY_AUDIT.md §5`](docs/security/SECURITY_AUDIT.md).
- **Prometheus metrics** — `/prometheus` endpoint exposes query / cache / connection / audit counters.
- **Master-replica replication** — WAL streaming, async / sync modes, automatic failover.
- **V2 sharded cluster (core)** — hash-partitioned shards with per-shard Raft consensus, distributed query coordinator, cross-shard traversal, `/cluster/*` management API. Opt-in via `[cluster.sharding]`. See [`docs/guides/DISTRIBUTED_DEPLOYMENT.md`](docs/guides/DISTRIBUTED_DEPLOYMENT.md).

## 🏗️ Architecture

```
┌───────────────────────────────────────────────────────────────────────┐
│                         Client transports                             │
│  Binary RPC (15475)   │    HTTP/JSON (15474)   │   RESP3 (15476)      │
│  nexus_protocol::rpc  │    axum routes         │   resp3 TCP listener │
└─────────┬─────────────┴───────────┬────────────┴───────────┬──────────┘
          │                         │                        │
          └─────────────┬───────────┴────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│                   Cypher Executor                        │
│        Pattern Match • Expand • Filter • Project         │
│         Heuristic Cost-Based Query Planner               │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│            Transaction Layer (MVCC)                      │
│      Epoch-Based Snapshots • Single-Writer Locking       │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│                   Index Layer                            │
│   Label Bitmap • B-tree • Full-Text • KNN (HNSW)         │
└───────────────────────┬──────────────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────────────┐
│                  Storage Layer                           │
│  Catalog (LMDB) • WAL • Record Stores • Page Cache       │
└──────────────────────────────────────────────────────────┘
```

| Layer                   | Tech                            | Purpose                                       |
|-------------------------|---------------------------------|-----------------------------------------------|
| Binary RPC listener     | `nexus_protocol::rpc` + tokio   | Default transport, multiplexed connections.  |
| HTTP router             | `axum`                          | REST + SSE + MCP + UMICP + GraphQL.          |
| RESP3 listener          | Hand-written parser             | `redis-cli`-compatible debug port.           |
| Catalog                 | LMDB via `heed`                 | Label / type / key ID mappings.              |
| Record stores           | `memmap2` fixed-size records    | 32-byte nodes, 48-byte relationships.        |
| Page cache              | Custom (Clock / 2Q / TinyLFU)   | 8 KB pages, hot-data retention.              |
| WAL                     | Append-only with CRC32C         | Crash recovery + replication stream.         |
| KNN                     | `hnsw_rs`                       | Per-label HNSW vector index.                 |
| Executor                | Custom                          | Cypher parser + planner + operators.         |

Full detail: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## 🗄️ Multi-database

```cypher
-- Via Cypher (CLI / SDK / HTTP all work)
SHOW DATABASES
CREATE DATABASE mydb
:USE mydb
RETURN database() AS current_db
DROP DATABASE mydb
```

```bash
# Via the CLI
nexus db list
nexus db create mydb
nexus db switch mydb
nexus db info
nexus db drop mydb
```

Every SDK ships equivalent methods (`list_databases` / `listDatabases` / etc.). Databases are fully isolated — separate catalogs, stores, and indexes. See [`docs/users/guides/MULTI_DATABASE.md`](docs/users/guides/MULTI_DATABASE.md) for the full lifecycle guide.

## 📦 Official SDKs

Six first-party SDKs, all tracking the same 1.0.0 line. Every SDK shares the URL grammar, command-map table, and error semantics defined in [`docs/specs/sdk-transport.md`](docs/specs/sdk-transport.md).

| SDK            | Install                                    | Docs                                          | RPC status   |
|----------------|--------------------------------------------|-----------------------------------------------|--------------|
| 🦀 Rust        | `nexus-sdk = "1.0.0"`                      | [sdks/rust/](sdks/rust/README.md)             | ✅ shipped   |
| 🐍 Python      | `pip install nexus-sdk`                    | [sdks/python/](sdks/python/README.md)         | ✅ shipped   |
| 📘 TypeScript  | `npm install @hivehub/nexus-sdk`           | [sdks/typescript/](sdks/typescript/README.md) | ✅ shipped   |
| 🐹 Go          | `go get github.com/hivellm/nexus-go`       | [sdks/go/](sdks/go/README.md)                 | ✅ shipped   |
| 💜 C#          | `dotnet add package Nexus.SDK`             | [sdks/csharp/](sdks/csharp/README.md)         | ✅ shipped   |
| 🐘 PHP         | `composer require hivellm/nexus-php`       | [sdks/php/](sdks/php/README.md)               | ✅ shipped   |

Every SDK defaults to `nexus://host:15475` (binary RPC) and falls back to HTTP/JSON when given an `http://` URL. RESP3 is a diagnostic port — SDKs refuse `resp3://` with a clear error pointing at the transport spec. Transport precedence across languages: **URL scheme > `NEXUS_SDK_TRANSPORT` env > config field > `nexus` default**.

### SDK quick examples

**Python**:
```python
from nexus_sdk import NexusClient

async with NexusClient("nexus://127.0.0.1:15475") as client:
    result = await client.execute_cypher("MATCH (n:Person) RETURN n.name LIMIT 10")
    print(f"Found {len(result.rows)} people via {client.endpoint_description()}")
```

**TypeScript**:
```typescript
import { NexusClient } from '@hivehub/nexus-sdk';

const client = new NexusClient({ baseUrl: 'nexus://127.0.0.1:15475' });
const result = await client.executeCypher('MATCH (n:Person) RETURN n.name LIMIT 10');
console.log(`Found ${result.rows.length} people`);
```

**Rust**:
```rust
use nexus_sdk::NexusClient;
let client = NexusClient::new("nexus://127.0.0.1:15475")?;
let result = client.execute_cypher("MATCH (n:Person) RETURN n.name LIMIT 10", None).await?;
```

## ⚙️ Configuration

### Environment variables (server)

| Var                           | Default              | Purpose                                    |
|-------------------------------|----------------------|--------------------------------------------|
| `NEXUS_ADDR`                  | `127.0.0.1:15474`    | HTTP bind address.                        |
| `NEXUS_RPC_ADDR`              | `127.0.0.1:15475`    | Binary RPC bind address.                  |
| `NEXUS_RESP3_ENABLED`         | `false`              | Enable RESP3 listener on `15476`.         |
| `NEXUS_DATA_DIR`              | `./data`             | Storage root (catalog + stores + WAL).    |
| `NEXUS_AUTH_ENABLED`          | `true` for `0.0.0.0` | Force API-key / RBAC authentication.      |
| `NEXUS_REPLICATION_ROLE`      | `standalone`         | `master`, `replica`, or `standalone`.     |
| `NEXUS_REPLICATION_MODE`      | `async`              | `async` or `sync`.                        |
| `NEXUS_SIMD_DISABLE`          | `0`                  | Emergency scalar fallback across kernels. |
| `RUST_LOG`                    | `info`               | Logging level (`error`/`warn`/`info`/`debug`/`trace`). |

### Environment variables (CLI / SDK)

| Var                      | Effect                                                           |
|--------------------------|------------------------------------------------------------------|
| `NEXUS_URL`              | Endpoint URL for the CLI (`nexus://`, `http://`, etc.).         |
| `NEXUS_TRANSPORT`        | Force CLI transport (`nexus` / `http` / `auto`).                |
| `NEXUS_SDK_TRANSPORT`    | Same semantics for SDKs.                                         |
| `NEXUS_API_KEY`          | API-key authentication (CLI).                                   |
| `NEXUS_USERNAME` / `NEXUS_PASSWORD` | Username/password authentication (CLI).               |

### Data directory layout

```
data/
├── catalog.mdb          # LMDB catalog (labels, types, keys)
├── nodes.store          # 32-byte node records
├── rels.store           # 48-byte relationship records
├── props.store          # Variable-size property records
├── strings.store        # String / blob dictionary
├── wal.log              # Write-ahead log (v1 / v2 frames)
├── checkpoints/         # Per-epoch snapshots
└── indexes/
    ├── label_*.bitmap   # Label bitmap indexes (RoaringBitmap)
    └── hnsw_*.bin       # HNSW vector indexes
```

## 🔐 Authentication

Auth is **disabled by default on 127.0.0.1** (local dev) and **required for 0.0.0.0 binding**. The first admin is the configured root user; every subsequent user is created via `CREATE USER` Cypher.

```bash
# Create the first user + API key via the CLI
nexus --username root --password root user create alice --password secret
nexus --username alice --password secret key create myapp

# Use the API key on later requests
nexus --api-key nexus_sk_abc123... query "RETURN 1"
```

Rate limiting: 1k requests/minute and 10k/hour per API key, returning `429 Too Many Requests` when exceeded. Full details: [`docs/security/AUTHENTICATION.md`](docs/security/AUTHENTICATION.md).

### RESP3 debug port

```bash
NEXUS_RESP3_ENABLED=true ./nexus-server
# in another shell:
redis-cli -p 15476
127.0.0.1:15476> HELLO 3 AUTH root root
127.0.0.1:15476> CYPHER "MATCH (n) RETURN count(n) AS c"
127.0.0.1:15476> STATS
```

Not Redis emulation — `SET key value` returns `-ERR unknown command 'SET' (Nexus is a graph DB, see HELP)`. Full vocabulary: [`docs/specs/resp3-nexus-commands.md`](docs/specs/resp3-nexus-commands.md).

## 🔄 Replication

Master-replica replication ships in 1.0.0. Use the master for writes, replicas for reads.

```bash
# Master
NEXUS_REPLICATION_ROLE=master \
NEXUS_REPLICATION_BIND_ADDR=0.0.0.0:15475 \
./nexus-server

# Replica
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=master:15475 \
./nexus-server
```

| Endpoint               | Method | Purpose                          |
|------------------------|--------|----------------------------------|
| `/replication/status`  | GET    | Role + lag + connected replicas. |
| `/replication/promote` | POST   | Promote a replica.               |
| `/replication/pause`   | POST   | Pause WAL streaming.             |
| `/replication/resume`  | POST   | Resume WAL streaming.            |
| `/replication/lag`     | GET    | Real-time lag metrics.           |

Features: full sync via snapshot, incremental WAL streaming (circular buffer, 1M ops), async / sync modes, auto-reconnect with exponential backoff.

## 🕸️ Sharded Cluster (V2)

Nexus V2 adds horizontal scalability through **hash-based sharding** with
**per-shard Raft consensus** and a **distributed query coordinator**.
Standalone deployments are unaffected — sharding is opt-in via
`[cluster.sharding]`.

```toml
# config.toml
[cluster.sharding]
mode = "bootstrap"        # "disabled" | "bootstrap" | "join"
node_id = "node-a"
listen_addr = "0.0.0.0:15480"
peers = [
  { node_id = "node-a", addr = "10.0.0.1:15480" },
  { node_id = "node-b", addr = "10.0.0.2:15480" },
  { node_id = "node-c", addr = "10.0.0.3:15480" },
]
num_shards = 3
replica_factor = 3
```

| Endpoint                  | Method | Purpose                                   |
|---------------------------|--------|-------------------------------------------|
| `/cluster/status`         | GET    | Layout + generation + per-shard health.  |
| `/cluster/add_node`       | POST   | Admin: register a node (leader-only).    |
| `/cluster/remove_node`    | POST   | Admin: remove a node, `drain: true` opt. |
| `/cluster/rebalance`      | POST   | Admin: trigger rebalancer.               |
| `/cluster/shards/{id}`    | GET    | Per-shard detail.                        |

Guarantees:

- **Leader election** ≤ 3× election timeout (~900 ms default).
- **Majority quorum replication** — 3-replica shard tolerates 1 loss,
  5-replica tolerates 2.
- **Atomic scatter/gather** — any shard failure fails the whole query
  (no partial rows).
- **Generation-tagged metadata** — stale coordinator caches detected
  via `ERR_STALE_GEN` and refreshed once before retry.
- **Leader-hint retry** — scatter RPCs retry up to 3× against the new
  leader on `ERR_NOT_LEADER`.

Full operator guide + failure-mode table:
[`docs/guides/DISTRIBUTED_DEPLOYMENT.md`](docs/guides/DISTRIBUTED_DEPLOYMENT.md).
Multi-host TCP transport between replicas is scheduled in the
[`phase5_v2-tcp-transport-bridge`](.rulebook/tasks/phase5_v2-tcp-transport-bridge/)
follow-up task; the current in-process transport covers single-host
clusters and all integration scenarios.

## 📦 Use cases

### RAG (Retrieval-Augmented Generation)
```cypher
CALL vector.knn('Document', $query_vector, 10)
YIELD node AS doc, score
MATCH (doc)-[:CITES]->(cited:Document)
RETURN doc.title, doc.content, COLLECT(cited.title) AS citations, score
ORDER BY score DESC
```

### Recommendation engine
```cypher
MATCH (user:Person {id: $user_id})-[:LIKES]->(item:Product)
MATCH (item)<-[:LIKES]-(similar:Person)
MATCH (similar)-[:LIKES]->(recommendation:Product)
WHERE NOT (user)-[:LIKES]->(recommendation)
RETURN recommendation.name, COUNT(*) AS score
ORDER BY score DESC LIMIT 10
```

### Code analysis + LLM context
```cypher
CALL graph.generate('call_graph', {scope: {file_patterns: ['*.rs']}})
YIELD graph_id
CALL graph.analyze(graph_id, 'pattern_detection')
YIELD pattern_type, confidence, nodes
WHERE confidence > 0.8
RETURN pattern_type, confidence, SIZE(nodes) AS pattern_size
ORDER BY confidence DESC
```

## 📖 Documentation

### User-facing
- [`docs/users/`](docs/users/README.md) — getting started, Cypher reference, API guide, SDKs, ops, tuning.
- [`docs/users/getting-started/`](docs/users/getting-started/) — installation, Docker, first query.
- [`docs/users/cypher/`](docs/users/cypher/) — supported Cypher grammar.
- [`docs/users/vector-search/`](docs/users/vector-search/) — HNSW + hybrid queries.
- [`docs/users/configuration/`](docs/users/configuration/) — server, logging, performance, cluster.

### Architecture + specs
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — system design.
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — development phases.
- [`docs/specs/`](docs/specs/) — binary specs (storage format, Cypher subset, page cache, WAL/MVCC, KNN, RPC wire format, RESP3 vocabulary, SDK transport, SIMD dispatch).

### Compatibility
- [`docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`](docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md) — **300/300** tests (Neo4j 2025.09.0 diff), per-feature breakdown.

### Performance
- [`docs/performance/PERFORMANCE_V1.md`](docs/performance/PERFORMANCE_V1.md) — throughput, latency, vs-Neo4j benchmark matrix.
- [`docs/performance/MEMORY_TUNING.md`](docs/performance/MEMORY_TUNING.md) — cache sizes, page budgets, Docker memtest.

## 🗺️ Roadmap

### 1.0.0 — current
- ✅ Property graph engine + ~55% openCypher + 19 GDS procedures.
- ✅ Native HNSW KNN per label.
- ✅ Binary RPC default, HTTP/JSON legacy, RESP3 debug.
- ✅ CLI + Rust SDK at RPC-first 1.0.0.
- ✅ Authentication (API keys, JWT, RBAC, rate limiting, audit).
- ✅ Multi-database.
- ✅ Master-replica replication (async / sync).
- ✅ SIMD kernels (AVX-512 / AVX2 / NEON) with runtime dispatch.
- ✅ 2310 workspace tests passing on `cargo +nightly test --workspace` (0 failed, 67 ignored); **300/300** Neo4j diff suite.
- ✅ **V2 sharding core** (2026-04-20) — hash-based shards, per-shard Raft consensus, distributed query coordinator, `/cluster/*` API. +201 V2 tests, 2169 total passing on the restructured `crates/` workspace.

### Queued on 1.x

Every item below is an active rulebook task under [`.rulebook/tasks/`](.rulebook/tasks/). Pick one up via `rulebook_task_show <taskId>`.

**openCypher coverage** (broadening Neo4j 5.x parity):
- 🧭 [`phase6_opencypher-constraint-enforcement`](.rulebook/tasks/phase6_opencypher-constraint-enforcement/) — `UNIQUE` / `NODE KEY` / `EXISTS` constraints with pre-commit hook.
- 🧭 [`phase6_opencypher-fulltext-search`](.rulebook/tasks/phase6_opencypher-fulltext-search/) — Tantivy-backed FTS, `db.index.fulltext.queryNodes`.
- 🧭 [`phase6_opencypher-quantified-path-patterns`](.rulebook/tasks/phase6_opencypher-quantified-path-patterns/) — Neo4j 5.9 QPP syntax (`()-[]->{1,5}()`).
- 🧭 [`phase6_opencypher-subquery-transactions`](.rulebook/tasks/phase6_opencypher-subquery-transactions/) — `CALL {} IN TRANSACTIONS OF N ROWS`.
- 🧭 [`phase6_opencypher-advanced-types`](.rulebook/tasks/phase6_opencypher-advanced-types/) — byte arrays, dynamic labels on writes, composite B-tree indexes.
- 🧭 [`phase6_opencypher-system-procedures`](.rulebook/tasks/phase6_opencypher-system-procedures/) — `db.schema.*`, `db.labels`, `db.indexes`, `db.indexDetails`.
- 🧭 [`phase6_opencypher-geospatial-predicates`](.rulebook/tasks/phase6_opencypher-geospatial-predicates/) — R-tree index + point / polygon predicates.
- 🧭 [`phase6_opencypher-quickwins`](.rulebook/tasks/phase6_opencypher-quickwins/) — type-checking predicates, `isEmpty` polymorphism, list converters, string extractors.
- 🧭 [`phase6_opencypher-apoc-ecosystem`](.rulebook/tasks/phase6_opencypher-apoc-ecosystem/) — APOC-compatible `apoc.coll.*` / `apoc.map.*` / `apoc.date.*` procedures.

**Infrastructure & ops**:
- 🧭 [`phase5_v2-tcp-transport-bridge`](.rulebook/tasks/phase5_v2-tcp-transport-bridge/) — multi-host TCP transport for V2 Raft replicas (current in-process transport covers single-host + integration tests).
- 🧭 [`phase5_implement-v1-gui`](.rulebook/tasks/phase5_implement-v1-gui/) — Electron + Vue 3 desktop app (`gui/` directory scaffolded; 0% implementation).
- 🧭 [`phase5_hub-integration`](.rulebook/tasks/phase5_hub-integration/) — HiveHub SaaS multi-tenant control plane (billing, per-user databases).
- 🧭 [`phase6_nexus-vs-neo4j-benchmark-suite`](.rulebook/tasks/phase6_nexus-vs-neo4j-benchmark-suite/) — Docker-based harness for apples-to-apples vs-Neo4j benchmarks.

**Protocol polish**:
- 🧭 **RESP3 in non-Rust SDKs** — every SDK accepts `resp3://` URLs and throws a clear error; parser/writer queued for a subsequent 1.x release.
- 🧭 **Streaming Cypher / live queries** — RPC push frame is reserved (`PUSH_ID = u32::MAX`); no SDK consumer yet.

### V2 — distributed ✅ core complete (2026-04-20)
- ✅ **Sharding** — deterministic xxh3 hash partitioning, generation-tagged metadata, iterative rebalancer ([`crates/nexus-core/src/sharding/`](crates/nexus-core/src/sharding/)).
- ✅ **Raft consensus** — purpose-built per-shard Raft with leader election, log replication, snapshot install, 5-node tolerates 2 failures ([`crates/nexus-core/src/sharding/raft/`](crates/nexus-core/src/sharding/raft/)).
- ✅ **Distributed queries** — scatter/gather coordinator with atomic failure, leader-hint retry, COUNT/SUM/AVG decomposition, top-k merge ([`crates/nexus-core/src/coordinator/`](crates/nexus-core/src/coordinator/)).
- ✅ **Cluster operations** — `/cluster/*` management API, admin-gated, 307 redirect on followers, drain semantics ([`crates/nexus-server/src/api/cluster.rs`](crates/nexus-server/src/api/cluster.rs)).
- 🧭 **Multi-host TCP transport** — `phase5_v2-tcp-transport-bridge` follow-up (current in-process transport covers single-host + all integration tests).
- 🧭 **Online re-sharding / cross-shard 2PC / geo-distribution** — V2.1 / V3.

Full detail: [`docs/ROADMAP.md`](docs/ROADMAP.md).

## ⚡ Why Nexus?

| Property         | Neo4j                | Redis / Redis Stack   | **Nexus**                                |
|------------------|----------------------|-----------------------|------------------------------------------|
| Query language   | Cypher (full)        | RedisGraph Cypher     | Cypher subset (20% = 80% coverage)       |
| Storage model    | Record stores        | In-memory / flash     | Record stores (Neo4j-inspired)           |
| Vector search    | GDS plugin           | Redis Search module   | **Native HNSW per label**                |
| Default transport| Bolt binary          | RESP3                 | **Binary RPC (MessagePack)**, HTTP, RESP3 |
| SDKs             | Official 7 languages | Extensive community   | 6 first-party, same 1.0.0 cadence        |
| Target workload  | General graph OLTP   | In-memory KV + graph  | **Read-heavy + RAG + vector hybrid**     |

### Sweet spot
- RAG with citation + semantic hybrid.
- Recommendation systems over labeled embeddings.
- Knowledge graphs with vector-seeded expansion.
- Code-analysis call graphs + pattern detection for LLM context.

### Not ideal
- Write-heavy OLTP — use a traditional RDBMS.
- Pure key-value — use Redis or Synap.
- Document-only search — use Elasticsearch or the Vectorizer standalone.

## 🛠️ Development

### Project layout

```
nexus/
├── crates/                # 🦀 Rust workspace crates (standard layout)
│   ├── nexus-core/        # 🧠 Graph engine library (catalog, storage, WAL,
│   │                      #    index, executor, MVCC, sharding, coordinator)
│   ├── nexus-server/      # 🌐 Axum HTTP + RPC + RESP3 server
│   ├── nexus-protocol/    # 🔌 Shared wire types (rpc, resp3, mcp, rest, umicp)
│   └── nexus-cli/         # 💻 `nexus` binary, RPC-default
├── sdks/                  # 📦 Six first-party SDKs (rust, python, typescript, go, csharp, php)
├── tests/                 # 🧪 Workspace integration + Neo4j compatibility
├── docs/                  # 📚 User guides, specs, compatibility, performance
└── scripts/install/       # 🔧 install.sh / install.ps1
```

### Build + test

```bash
cargo +nightly build --release --workspace
cargo +nightly test --workspace
cargo +nightly clippy --workspace --all-targets --all-features -- -D warnings
cargo +nightly fmt --all
```

Coverage: `cargo llvm-cov --workspace --ignore-filename-regex 'examples'` (95%+ expected for new code).

## 🤝 Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the workflow. Short version:

1. Fork, branch (`git checkout -b feature/your-feature`).
2. Make changes + tests (95%+ coverage on new code).
3. Run `cargo fmt`, `cargo clippy`, `cargo test`.
4. Conventional commits (`feat(scope): …`, `fix(scope): …`, `refactor(scope): …`).
5. Submit PR with description, tests, docs.

For major features, this repo uses [@hivehub/rulebook](https://github.com/hivellm/rulebook) for spec-driven development — proposals + tasks live under [`.rulebook/tasks/`](.rulebook/tasks/).

## 📜 License

Apache-2.0. See [`LICENSE`](LICENSE).

## 🙏 Acknowledgments

- [Neo4j](https://neo4j.com/) — inspiration for the record-store layout and Cypher.
- [HNSW](https://arxiv.org/abs/1603.09320) — the approximate-NN algorithm powering KNN.
- [OpenCypher](https://opencypher.org/) — language specification.
- [Rust](https://www.rust-lang.org/) — the ecosystem behind every crate here.

## 📞 Contact

- 🐛 Issues: [github.com/hivellm/nexus/issues](https://github.com/hivellm/nexus/issues)
- 💬 Discussions: [github.com/hivellm/nexus/discussions](https://github.com/hivellm/nexus/discussions)
- 📧 Email: team@hivellm.org

---

<div align="center">

**Built with ❤️ in Rust** 🦀

_Neo4j-compatible graph + native vector search, binary-RPC first_

[⭐ Star on GitHub](https://github.com/hivellm/nexus) • [📖 Docs](docs/) • [🚀 Quick Start](#-quick-start)

</div>
