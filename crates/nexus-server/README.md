# nexus-server

HTTP / MCP / GraphQL / RPC server for the Nexus graph database.
Thin transport shell on top of [`nexus-core`](../nexus-core); all
storage, query execution, and indexing live there. This crate
wires HTTP routes, auth middleware, rate limiting, and the binary
RPC listener.

## Endpoints

### Cypher & data

| Method · Path | Purpose |
|---|---|
| `POST /cypher` | Execute Cypher (`{ "query": "...", "parameters": {...} }`). |
| `POST /knn_traverse` | KNN-seeded graph traversal. |
| `POST /ingest` | Bulk ingestion. |
| `POST /export` | Stream graph data out. |
| `POST /streaming/*` | Server-sent Cypher result streams. |

### Schema

| Method · Path | Purpose |
|---|---|
| `GET /schema/labels` · `POST /schema/labels` | List / create labels. |
| `GET /schema/rel_types` · `POST /schema/rel_types` | List / create relationship types. |
| `GET /schema/property_keys` | List property keys. |
| `GET /indexes` · `POST /indexes` | Manage indexes. |

### Multi-database

| Method · Path | Purpose |
|---|---|
| `GET /databases` | List databases. |
| `POST /databases` | Create database (`{ "name": "..." }`). |
| `DELETE /databases/{name}` | Drop database. |
| `PUT /session/database` | Switch session database. |

### Auth (RBAC + JWT + API keys)

| Method · Path | Purpose |
|---|---|
| `POST /auth/login` | Username + password → JWT. |
| `POST /auth/api-keys` | Issue API key. |
| `GET /auth/users` · `POST /auth/users` | RBAC user management. |

### Operational

| Method · Path | Purpose |
|---|---|
| `GET /health` | Liveness probe. |
| `GET /stats` | Engine statistics. |
| `GET /metrics` | Prometheus exposition. |
| `GET /openapi.json` | OpenAPI 3.1 spec. |
| `GET /debug/memory` | Heap profiling (requires `memory-profiling` feature). |
| `GET /performance/*` | Per-query stats, plan cache hit rate, DBMS procedures. |
| `POST /mcp` | Model Context Protocol StreamableHTTP endpoint. |
| `POST /graphql` · `GET /graphql` | GraphQL endpoint + playground (`async-graphql`). |
| `*` cluster / replication / clustering | V2 distributed control plane. |

The full machine-readable surface is at `GET /openapi.json` once
the server is running.

### Default response shape

```json
{
  "columns": ["n.name", "n.age"],
  "rows": [
    ["Alice", 30],
    ["Bob", 25]
  ],
  "execution_time_ms": 3,
  "stats": {
    "nodes_created": 0,
    "relationships_created": 0,
    "properties_set": 0
  }
}
```

`rows` is **always** an array of arrays (Neo4j-compatible). Never
an array of objects. SDK helpers (`RowsAsMap()`) convert
client-side.

## Listeners

| Listener | Default | Source |
|---|---|---|
| HTTP / WebSocket / GraphQL / MCP | `127.0.0.1:15474` | `axum` |
| Binary RPC (length-prefixed MessagePack) | `127.0.0.1:15475` | `protocol/rpc/` |

Override with `NEXUS_ADDR`, `NEXUS_RPC_ADDR`, or via
`config.yml` / `--config <path>`.

## Build & run

```bash
cargo +nightly build --release -p nexus-server
./target/release/nexus-server                     # default config
./target/release/nexus-server --config config.yml # explicit config
NEXUS_ADDR=0.0.0.0:15474 NEXUS_DATA_DIR=./data \
    ./target/release/nexus-server                 # env override

# Memory profiling build (jemalloc + pprof dump on /debug/memory).
cargo +nightly build --release -p nexus-server --features memory-profiling
```

The Linux Debian package is built from the same crate via
`cargo deb` (see `[package.metadata.deb]` in `Cargo.toml` and
`debian/`).

## Test

```bash
cargo +nightly test -p nexus-server
cargo +nightly test --workspace                   # full suite (2310 passing)

cargo +nightly clippy -p nexus-server --all-targets --all-features -- -D warnings
cargo +nightly fmt --all
```

Server integration tests live under `tests/`. Cluster /
replication paths are gated behind their own scenarios; check
`tests/` for the canonical flows.

## Module map

| Path | Purpose |
|---|---|
| `main.rs` · `lib.rs` | Binary entry point + `NexusServer` shared state. |
| `api/` | One module per route group (`cypher/`, `data`, `schema`, `auth`, `database`, `health`, `stats`, `metrics`, `prometheus`, `openapi`, `graphql/`, `streaming`, `clustering`, `replication`, `comparison`, `graph_correlation`, …). |
| `middleware/` | `auth`, `mcp_auth`, `rate_limit`, `admission`. |
| `protocol/` | Binary RPC listener (`rpc/`) + RESP3 codec (`resp3/`). |
| `cluster_bootstrap.rs` | Boots cluster mode if `cluster.enabled = true`. |
| `config.rs` | YAML / TOML config loader (`config.yml`, `config/auth.toml`). |

## Cargo features

| Feature | Default | Effect |
|---|---|---|
| `memory-profiling` | off | Switch global allocator to jemalloc with pprof hooks; expose `/debug/memory`. Off by default — production builds do not pay the allocator switch cost. |

## Auth defaults

- **Localhost (`127.0.0.1`)**: auth disabled by default. Drop-in
  developer experience.
- **Public bind (`0.0.0.0`)**: auth required. Server refuses to
  start without an admin credential configured.
- **API keys**: 32-char random, Argon2-hashed at rest.
- **JWT**: HS256 via `jsonwebtoken` (`rust_crypto` backend, no
  system OpenSSL dep). Configurable lifetime + refresh.
- **Rate limits**: 1000 req/min, 10 000 req/hour per API key
  (`middleware/rate_limit.rs`).

Full setup: [`docs/security/AUTHENTICATION.md`](../../docs/security/AUTHENTICATION.md).

## Hard constraints

- **`await_holding_lock = "deny"`** is enforced at the crate
  level. The `DatabaseManager` lock must never be held across
  `.await`. If you need to await with a lock open, drop it and
  re-acquire, or move the work into `tokio::task::spawn_blocking`.
  See [`docs/performance/CONCURRENCY.md`](../../docs/performance/CONCURRENCY.md).
- **No `unwrap()` in `main.rs` / binary entry points** outside
  `#[cfg(test)]` modules. Use `?` + `anyhow::Context`. Enforced
  by `scripts/ci/check_no_unwrap_in_bin.sh` on every CI run.
- **Wire format is fixed.** `rows` is always
  `[[v1, v2]]`. Do not add object-of-columns variants.
- **Server response time targets**: <1 ms point reads, <2 ms KNN
  queries (p95). `cargo bench -p nexus-core protocol_point_read`
  is the reference probe.

## Links

- Architecture: [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md)
- API protocols: [`docs/specs/api-protocols.md`](../../docs/specs/api-protocols.md)
- Authentication: [`docs/security/AUTHENTICATION.md`](../../docs/security/AUTHENTICATION.md)
- Concurrency rules: [`docs/performance/CONCURRENCY.md`](../../docs/performance/CONCURRENCY.md)
- Engine library: [`crates/nexus-core`](../nexus-core)
- Wire-protocol clients: [`crates/nexus-protocol`](../nexus-protocol)
- CLI: [`crates/nexus-cli`](../nexus-cli)
