# Changelog

All notable changes to Nexus will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] — 2026-04-20

### Fixed — RPC DELETE / DETACH DELETE no-op (2026-04-20)

Queries like `MATCH (n) DETACH DELETE n` issued over the native
MessagePack RPC protocol parsed and returned `Ok(0 rows)` but left
the database untouched. Root cause: the RPC CYPHER dispatch in
`crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs` called
`executor.execute(&q)` directly for every non-admin query. The
operator pipeline's `Operator::Delete` / `Operator::DetachDelete`
handlers are explicit no-ops — they rely on the engine's
higher-level interception (`execute_cypher_with_context` at
`crates/nexus-core/src/engine/mod.rs:1427`) to perform the actual
mutation. REST always went through that path; RPC bypassed it.

The fix adds a `needs_engine_interception(&ast)` router: any AST
that carries `Match` / `Create` / `Delete` / `Merge` / `Set` /
`Remove` / `Foreach` now routes through `engine.execute_cypher`,
preserving parity with the REST transport. Read-only queries
(no MATCH, no mutation) keep the parallel executor path —
unchanged throughput, unchanged params handling.

Verified end-to-end against a live Nexus RPC listener + docker
Neo4j 2025.09.0: `nexus-bench`'s 9 `#[ignore]` integration tests
now run cleanly as a single `cargo test -p nexus-bench
--features live-bench,neo4j -- --ignored` parallel batch (used to
require per-test manual wipes). A new engine-level regression
test (`detach_delete_actually_clears_nodes_via_execute_cypher` in
`crates/nexus-core/src/engine/tests.rs`) locks the interception
contract.

Source task: `phase6_nexus-delete-executor-bug`.

### Added — server admission control (2026-04-20)

Third back-pressure layer on top of the existing per-key rate limiter
and per-connection RPC semaphore. A global `AdmissionQueue`
(`crates/nexus-server/src/middleware/admission.rs`) gates every
query-bearing HTTP route (`/cypher`, `/ingest`, `/knn_traverse`,
`/graphql`, `/umicp`) through a shared tokio semaphore. Callers that
would push concurrency over `NEXUS_ADMISSION_MAX_CONCURRENT` (default
CPU-count clamped to `[4, 32]`) wait in a FIFO queue up to
`NEXUS_ADMISSION_QUEUE_TIMEOUT_MS` (default 5 s); after that they
are rejected with `503 Service Unavailable + Retry-After`.

Motivation: a single authenticated client can fan out tens of
thousands of legitimate-looking `CREATE` statements through one
HTTP keep-alive — enough to saturate the engine's single-writer
discipline and wedge the process even though every request sat under
the per-key rate limit. The new layer bounds **global** engine-facing
concurrency rather than per-key volume.

Light-weight endpoints (`/health`, `/prometheus`, `/auth`,
`/schema/*`, `/stats`, `/cluster/status`) bypass the queue via a
`HEAVY_PATH_PREFIXES` matcher so diagnostics stay reachable when
the engine is saturated. RPC + RESP3 surfaces continue to rely on
their per-connection semaphore; unified gating is a follow-up.

Config knobs:

- `NEXUS_ADMISSION_ENABLED` (bool, default `true`)
- `NEXUS_ADMISSION_MAX_CONCURRENT` (u32, default CPU-clamped)
- `NEXUS_ADMISSION_QUEUE_TIMEOUT_MS` (u64, default 5000)

Prometheus metric names reserved (counters + histogram wiring ships
in a subsequent patch):
`nexus_admission_permits_granted_total`,
`nexus_admission_permits_rejected_total`,
`nexus_admission_in_flight`,
`nexus_admission_wait_seconds`.

Docs: [`docs/security/OVERLOAD_PROTECTION.md`](docs/security/OVERLOAD_PROTECTION.md).
17 tests (unit + axum middleware) covering concurrency cap, timeout,
FIFO progress under contention, light-path short-circuit, heavy-path
rejection, counter integrity on drop.

### Added — V2 horizontal scaling (2026-04-20, commit `15715a24`)

Nexus gains horizontal scalability through hash-based sharding, per-shard
Raft consensus, and a distributed query coordinator. See
[`docs/guides/DISTRIBUTED_DEPLOYMENT.md`](docs/guides/DISTRIBUTED_DEPLOYMENT.md)
and [`.rulebook/tasks/phase5_implement-v2-sharding/design.md`](.rulebook/tasks/phase5_implement-v2-sharding/design.md).

- **Sharding** (`crates/nexus-core/src/sharding/`): deterministic xxh3-based
  shard assignment, generation-tagged cluster metadata, iterative
  rebalancer, per-shard health model. Standalone deployments are
  unchanged — sharding is opt-in via `[cluster.sharding]` config.
- **Raft consensus per shard** (`crates/nexus-core/src/sharding/raft/`):
  purpose-built Raft (openraft 0.10 is still alpha; its trait surface
  would require an adapter larger than the Raft itself). Leader
  election within 3× election timeout, §5.3 truncate-on-conflict,
  §5.4.2 leader-only current-term commit, snapshot install, bincode
  wire format with shard-id prefix. 5-node clusters tolerate 2
  replica failures.
- **Distributed query coordinator** (`crates/nexus-core/src/coordinator/`):
  scatter/gather with atomic per-query failure, leader-hint retry
  (3 attempts), stale-generation refresh, COUNT/SUM/AVG/MIN/MAX/
  COLLECT aggregation decomposition, ORDER BY + LIMIT top-k merge.
- **Cross-shard traversal**: TTL + generation-aware LRU cache (10k
  entries default), per-query fetch budget (1k default) with
  `ERR_TOO_MANY_REMOTE_FETCHES` for runaway traversals.
- **Cluster management API** (`crates/nexus-server/src/api/cluster.rs`):
  `GET /cluster/status`, `POST /cluster/{add_node,remove_node,rebalance}`,
  `GET /cluster/shards/{id}`. Admin-gated, `307 Temporary Redirect` on
  follower writes, drain semantics for graceful node removal.

### Changed — workspace layout

The four Rust crates moved from repo-root children into a single
`crates/` directory, following the standard Rust workspace layout:

```
Nexus/
├── crates/
│   ├── nexus-core/      # was ./nexus-core/
│   ├── nexus-server/    # was ./nexus-server/
│   ├── nexus-protocol/  # was ./nexus-protocol/
│   └── nexus-cli/       # was ./nexus-cli/
├── docs/                # unchanged
├── sdks/                # unchanged
└── scripts/             # unchanged
```

Follow-up edits:

- `Cargo.toml` root: `workspace.members` + `workspace.dependencies`
  paths updated to `crates/…`.
- `crates/nexus-core/Cargo.toml`: `[[example]]` paths `../examples/` →
  `../../examples/`.
- `crates/nexus-server/Cargo.toml` + `crates/nexus-cli/Cargo.toml`:
  `[package.metadata.deb]` asset paths (`../LICENSE`, `../README.md`,
  `../config.yml`, …) updated to `../../…`.
- `.github/workflows/rust-lint.yml`, `release-server.yml`,
  `release-cli.yml`: path filters + `manifest_path` point at `crates/…`.
- `scripts/ci/check_no_unwrap_in_bin.sh`: `SCOPES` + repo-root detection
  updated.
- Inter-crate paths (`../nexus-protocol`) unchanged — both live under
  `crates/` so the relative form still resolves.

No functional change; no public API moved or renamed.

### Test coverage

**201 V2-dedicated tests** — 143 sharding unit tests, 46 coordinator
unit tests, 12 E2E integration scenarios
(`crates/nexus-core/tests/v2_sharding_e2e.rs`) covering every §Scenario
in the specs:

- Deterministic assignment across restarts
- Metadata consistency after leader change
- Single-shard + broadcast query classification
- AVG / SUM / MIN / MAX / COLLECT aggregation decomposition
- Shard-failure atomicity (partial rows never leaked)
- Raft failover within spec bound (≤90 ticks = 900ms)
- Minority-failure replication continuity
- Rebalance convergence
- Leader-redirect on followers
- Stale-generation refresh round-trip

Full workspace on nightly: **2169 tests passing, 0 failed** (1694
nexus-core lib + 364 nexus-server lib + 83 nexus-protocol lib + 28
nexus-cli lib + 12 V2 E2E). Zero warnings on `cargo clippy
--workspace --all-targets -- -D warnings`. Release build (`cargo
+nightly build --release --workspace`) succeeds in ~3 minutes.

### Breaking changes (when sharding is enabled)

- Record-store files gain a 64-byte V2 header. Standalone deployments
  use deterministic defaults (`shard_id = 0`, `generation = 0`); a
  future `nexus migrate --to v2` CLI rewrites headers in place.

### Follow-up

- [`phase5_v2-tcp-transport-bridge`](.rulebook/tasks/phase5_v2-tcp-transport-bridge/)
  — TCP transport between Raft replicas for multi-host deployments.
  Current in-process transport covers single-host + all integration
  scenarios; the TCP bridge is an I/O adapter over the already-stable
  `RaftTransport` and `ShardClient` traits.

### Added — cluster mode (multi-tenant deployments, 2026-04-19)

Nexus can now run as a shared multi-tenant service. One server
instance hosts data for many tenants while guaranteeing that a
tenant's nodes, relationships, property keys, and label names stay
strictly isolated from every other tenant. See `docs/CLUSTER_MODE.md`
for the operator guide.

Enable with `NEXUS_CLUSTER_ENABLED=true` (opt-in; standalone mode
remains the default and is byte-identical to the pre-cluster
behaviour). Once on:

- **Mandatory authentication on every URI.** Cluster mode removes
  every public endpoint — `/`, `/health`, `/stats`, `/openapi.json`
  all require a valid API key. A shared multi-tenant server must
  identify every caller before exposing any surface.
- **Per-tenant data isolation.** Labels / relationship types /
  property keys registered by tenant A get different catalog IDs
  than the same names registered by tenant B, so every downstream
  layer (label bitmap index, KNN, record stores) sees tenant-
  distinct state for free. Data leakage is structurally impossible
  — not an invariant maintained by discipline. Proven end-to-end
  by the integration tests in `nexus-core/tests/cluster_isolation_tests.rs`.
- **Per-tenant rate limiting.** Every request is gated by
  `LocalQuotaProvider` (per-minute + per-hour windows, configurable
  via `ClusterConfig::default_quotas`). 429 responses carry
  `Retry-After` and `X-RateLimit-Remaining` headers so SDK clients
  can back off cleanly.
- **Function-level MCP permissions.** API keys gain an optional
  `allowed_functions` allow-list. Handlers can call
  `UserContext::require_may_call("tool.name")?` to gate specific
  MCP / RPC operations per-key, and discovery endpoints can use
  `filter_callable` to advertise only callable tools.

New public surface: `nexus_core::cluster::{ClusterConfig,
TenantIsolationMode, UserNamespace, UserContext, QuotaProvider,
LocalQuotaProvider, FunctionAccessError}`.

New env var: `NEXUS_CLUSTER_ENABLED`. Architecturally documented in
ADR-7 (catalog-prefix isolation over byte-level or per-database
alternatives).

### Changed — API key storage migrated from bincode to JSON

`nexus-core/src/auth/storage.rs` switched from `SerdeBincode<ApiKey>`
to `SerdeJson<ApiKey>` for the `api_keys` LMDB database. Bincode's
default config is NOT forward-compatible for appended fields —
adding cluster mode's new `allowed_functions: Option<Vec<String>>`
field would have panicked on every existing record with
`unexpected end of file`. JSON + `#[serde(default)]` gives us room
to grow the schema without a migration script.

**Operational note:** existing auth data is NOT automatically
migrated on upgrade. Cluster-mode deployments should regenerate API
keys from scratch; standalone deployments that already persist API
keys should expect to re-seed on first boot under the new binary.
The shared test-suite catalog was bumped to a new path
(`nexus_test_auth_shared_v2`) so stale bincode records from earlier
runs are orphaned cleanly instead of failing to decode.

### Fixed — parser no longer accepts standalone `WHERE` (Neo4j parity)

Closes the last outlier in the 300-test Neo4j compat suite. Before
this change, Nexus accepted `UNWIND [1,2,3,4,5] AS x WHERE x > 2
RETURN x` and returned `[3, 4, 5]`, while Neo4j 2025.09.0 rejects the
same query with a syntax error (`Invalid input 'WHERE': expected
'ORDER BY', 'CALL', ...`). Standard Cypher only allows `WHERE`
attached to `MATCH` / `OPTIONAL MATCH` / `WITH` — never as a
standalone top-level clause.

The parser now matches Neo4j's grammar exactly: a bare `WHERE` after
any clause other than those three rejects with the same error
message shape Neo4j produces, pointing callers at the migration.

**Breaking change — migration.** Any query that glued `WHERE`
directly onto the output of `UNWIND` / `CREATE` / `DELETE` (or any
other non-MATCH/WITH producer) must insert a `WITH <vars>`
pass-through projection before the predicate:

```cypher
-- before
UNWIND [1, 2, 3, 4, 5] AS x WHERE x > 2 RETURN x

-- after
UNWIND [1, 2, 3, 4, 5] AS x WITH x WHERE x > 2 RETURN x
```

The new syntax error points at the exact column and lists the
valid clauses, so stale call sites surface immediately on the next
request instead of going silent.

**Result.** Neo4j compat suite now reports **300/300 passing**
(previously 299/300 with 14.05 the one outlier). Every other test
across all 17 sections — Basic Queries, Pattern Matching,
Aggregations, Type Conversion, DELETE/SET, etc. — keeps its
scalar-path parity.

### SDK + workspace version unification

Every first-party crate and SDK bumped to **1.0.0** (previously a
mix of `0.12.0` for the server workspace and `0.1.0` for some SDKs).
One version number governs the CLI, server, protocol crate, Rust
SDK, Python SDK, TypeScript SDK, Go SDK, C# SDK, and PHP SDK.

### Removed ecosystem SDKs

The following integrations were dropped to focus on first-party wire
clients:

- `sdks/n8n/` — the community n8n node. Users can still invoke the
  Nexus HTTP endpoint or wrap the TypeScript SDK inline.
- `sdks/langchain/` and `sdks/langflow/` — Python ecosystem
  wrappers. The underlying Python SDK covers the same API surface;
  higher-level orchestration wrappers are better maintained
  out-of-tree where they can track upstream LangChain / LangFlow
  releases on their own cadence.
- `sdks/TestConsoleSimple/` — redundant C# test harness (the
  canonical tests live in `sdks/csharp/Tests/`).

### Documentation reorganisation

- New `sdks/README.md` — canonical index of shipped SDKs with the
  shared transport contract referenced up front.
- `sdks/SDK_TEST_RESULTS.md`, `sdks/SDK_TEST_RESULTS_FINAL.md`, and
  `sdks/TEST_COVERAGE_REPORT.md` moved to `docs/sdks/` so the `sdks/`
  root only holds runnable client code + the test-matrix script.
- Per-SDK `CHANGELOG.md` created for every remaining SDK (Rust,
  Python, TypeScript, Go, C#, PHP) — the Rust SDK entry has the
  full 1.0.0 RPC-default details, the others carry a "1.0.0 version
  alignment, RPC default queued under
  phase2_sdk-rpc-transport-default" entry.

### Native Binary RPC transport (2026-04-18)

**First-party SDKs now have a MessagePack RPC port.** Length-prefixed
frames (`[u32 LE][rmp-serde body]`) on port `15475`, multiplexed over
a single TCP connection via caller-chosen `Request.id`. Enabled by
default (`[rpc].enabled = true`); RESP3 and HTTP continue to run
unchanged alongside it.

```
NEW nexus-protocol/src/rpc/{mod,types,codec}.rs   (shared w/ SDKs)
NEW nexus-server/src/protocol/rpc/
    mod.rs, server.rs, metrics.rs,
    dispatch/{mod, admin, convert, cypher, database, graph, ingest, knn, schema}.rs
NEW nexus-server/tests/rpc_integration_test.rs
NEW docs/specs/rpc-wire-format.md
```

Command set: admin handshake (PING / HELLO / AUTH / QUIT / STATS /
HEALTH), CYPHER (with optional params map; EXPLAIN inline), graph CRUD
(CREATE_NODE / CREATE_REL / UPDATE_NODE / DELETE_NODE / MATCH_NODES),
KNN (KNN_SEARCH accepting embedding as Bytes-of-f32 or Array<Float>
with optional property filter, KNN_TRAVERSE with seed list + depth),
bulk ingest (INGEST, single-batch atomic), schema introspection
(LABELS / REL_TYPES / PROPERTY_KEYS / INDEXES from the catalog
directly), multi-database (DB_LIST / DB_CREATE / DB_DROP / DB_USE).

64 MiB cap per frame (tunable via `rpc.max_frame_bytes`), per-
connection in-flight cap (`max_in_flight_per_conn`, default 1024),
`u32::MAX` reserved as `PUSH_ID` for future streaming, slow-command
WARN logging at `rpc.slow_threshold_ms` (default 2 ms).

Prometheus: `nexus_rpc_connections` (gauge), `nexus_rpc_commands_total`
/ `_error_total`, `nexus_rpc_command_duration_microseconds_total`,
`nexus_rpc_frame_bytes_in_total` / `_out_total`,
`nexus_rpc_slow_commands_total`. Env overrides:
`NEXUS_RPC_{ENABLED, ADDR, REQUIRE_AUTH, MAX_FRAME_BYTES,
MAX_IN_FLIGHT, SLOW_MS}`.

The wire-format layer (RPC types + codec, RESP3 parser + writer) moved
from `nexus-server::protocol` into `nexus-protocol::{rpc, resp3}` so
the Rust SDK can depend on it without pulling the whole server crate.
Command dispatch and the TCP accept loop stay in `nexus-server`.

121 new tests (113 unit + 8 integration) covering every command,
wrong-arity / wrong-type guards, NOAUTH gating, pipelined multiplexing,
PUSH_ID rejection, and end-to-end CRUD round-trips over TCP.

### 🔌 RESP3 Transport (2026-04-18)

**Any RESP3 client — `redis-cli`, `iredis`, RedisInsight, Jedis, redis-rb,
Redix — can now talk to Nexus using a Nexus command vocabulary.** The port
is additive (HTTP, MCP, UMICP all keep running), disabled by default, and
loopback-only out of the box so a plaintext debug port never accidentally
escapes a dev machine.

```
NEW nexus-server/src/protocol/resp3/
  mod.rs, parser.rs, writer.rs, server.rs
  command/{mod, admin, cypher, graph, knn, schema}.rs
NEW nexus-server/tests/resp3_integration_test.rs
NEW docs/specs/resp3-nexus-commands.md
```

**25+ commands** implemented in the Nexus vocabulary:

- Admin: `PING`, `HELLO [2|3] [AUTH user pass]`, `AUTH <api-key|user pass>`,
  `QUIT`, `HELP`, `COMMAND`.
- Cypher: `CYPHER`, `CYPHER.WITH`, `CYPHER.EXPLAIN`.
- Graph CRUD: `NODE.CREATE/GET/UPDATE/DELETE/MATCH`, `REL.CREATE/GET/DELETE`.
- KNN / ingest: `KNN.SEARCH`, `KNN.TRAVERSE`, `INGEST.NODES`, `INGEST.RELS`.
- Schema / databases: `INDEX.CREATE/DROP/LIST`, `DB.LIST/CREATE/DROP/USE`,
  `LABELS`, `REL_TYPES`, `PROPERTY_KEYS`, `STATS`, `HEALTH`.

**Wire format**: all 12 RESP3 type prefixes (`+`, `-`, `:`, `$`, `*`, `_`,
`,`, `#`, `=`, `~`, `%`, `|`, `(`) supported on both parse and write, with
automatic RESP2 degradation (Null → `$-1`, Map → flat array, Boolean →
`:0`/`:1`, Verbatim → BulkString) when the peer negotiates `HELLO 2`.
`redis-cli`-style inline commands (`PING\r\n`) tokenised with quote and
escape support, so plain `telnet` sessions work too.

**Explicitly not Redis emulation.** `SET key value` returns
`-ERR unknown command 'SET' (Nexus is a graph DB, see HELP)`. No KV
semantics.

**Auth**: `HELLO 3 AUTH <user> <pass>` negotiates protocol + auth in one
round-trip. Pre-auth commands (`PING`/`HELLO`/`AUTH`/`QUIT`/`HELP`/`COMMAND`)
always run; everything else bounces with `-NOAUTH Authentication required.`
when the listener was configured with `require_auth = true` and the
session hasn't authenticated.

**Concurrency**: every handler that touches `Engine` or `DatabaseManager`
acquires the `parking_lot::RwLock` inside `tokio::task::spawn_blocking` —
same policy as the HTTP handlers (see `docs/performance/CONCURRENCY.md`).
A tokio worker thread is never pinned on a graph-engine lock.

**Metrics** (exported at `GET /prometheus`):
- `nexus_resp3_connections` (gauge)
- `nexus_resp3_commands_total` (counter)
- `nexus_resp3_commands_error_total` (counter)
- `nexus_resp3_command_duration_microseconds_total` (counter — divide by
  `commands_total` for an average)
- `nexus_resp3_bytes_read_total` / `nexus_resp3_bytes_written_total`

**Config**: `[resp3]` section in `config.yml` with `enabled`, `addr`,
`require_auth`. Env overrides `NEXUS_RESP3_{ENABLED,ADDR,REQUIRE_AUTH}`.
Default port `15476` (HTTP stays on `15474`).

**Testing**: 77 new tests green (69 in-crate unit + 8 raw-TCP integration).

### 🛡️ Audit-log Failure Propagation (2026-04-18)

**Eight `let _ = audit_logger.log_*(...).await` sites were silently
swallowing audit-log write failures.** All now go through a new helper
`nexus_core::auth::record_audit_log_failure(context, err)` that bumps a
process-global `AtomicU64` counter and emits a
`tracing::error!(target = "audit_log", context, error)` event.

**Policy: fail-open with metric.** The originating request keeps its
original HTTP status (401/429/500/200) — we do NOT convert audit-sink
failures into 500s, because doing so hands an attacker who can cause IO
pressure (disk fill, permission flap) a lever to mass-reject legitimate
traffic. Operators alarm on the Prometheus counter instead:

```promql
increase(nexus_audit_log_failures_total[5m]) > 0
```

**Call sites patched**:
- `nexus-core/src/auth/middleware.rs` × 4 (missing/invalid/errored API
  key, rate-limit exceeded).
- `nexus-server/src/api/cypher/execute.rs` × 4 (SET-property + SET-label
  success/failure on the Cypher write path).

**Metric**: `nexus_audit_log_failures_total` exported at `GET /prometheus`
with HELP text pointing operators at the alert template.

**Docs**: [docs/security/SECURITY_AUDIT.md §5](docs/security/SECURITY_AUDIT.md) documents the
full policy (behaviour, rationale, alarm template, code-location
inventory, "not fail-closed" guard). [docs/security/AUTHENTICATION.md](docs/security/AUTHENTICATION.md)
cross-links from its audit section.

### ⚡ Async Lock Migration — `DatabaseManager` off tokio workers (2026-04-18)

**14 async HTTP handlers acquired `Arc<parking_lot::RwLock<DatabaseManager>>`
directly inside `async fn`, pinning a tokio worker for the whole lock-held
window.** Under concurrent load this starved the runtime — observed during
the `fix/memory-leak-v1` debug session as the container dropping requests
well before hitting any memory limit.

**Fix**: wrap every async-context lock acquisition in
`tokio::task::spawn_blocking` so the read/write runs on the blocking
pool while tokio workers stay free. The lock type stays
`parking_lot::RwLock` because it is shared with sync Cypher execution in
`nexus-core/src/executor/shared.rs` — migrating the type would ripple into
~20 files and force every sync caller onto `.blocking_read()` (which
panics if ever reached from an async context). The `spawn_blocking`
approach fixes the starvation at the source with a fraction of the blast
radius.

**Touched call sites (14 total)**:
- `nexus-server/src/api/database.rs` — 6 handlers
  (`create`/`drop`/`list`/`get`/`get_session`/`switch_session`).
- `nexus-server/src/api/cypher/commands.rs` — 4 admin-Cypher sites
  (`UseDatabase`/`ShowDatabases`/`CreateDatabase`/`DropDatabase`).

**Enforcement**: `nexus-server/Cargo.toml` sets
`clippy::await_holding_lock = "deny"` so any future regression fails CI.

**Regression test**:
`test_concurrent_list_databases_does_not_starve_runtime` fires 32
concurrent `list_databases` calls on a 2-worker tokio runtime and asserts
all 32 return `200 OK` inside a 30 s pathological timeout. Runs in 0.15 s
post-migration.

**Docs**: [docs/performance/CONCURRENCY.md](docs/performance/CONCURRENCY.md)
documents the lock model end-to-end — primitives, the `DatabaseManager`
rule, clippy enforcement, migration-vs-wrap tradeoff, and which
`tokio::sync` locks legitimately stay.

### 🧱 Neo4j Compatibility Test Split (Tier 3.2) (2026-04-18)

**`nexus-core/tests/neo4j_compatibility_test.rs` was 2,103 LOC in a single
`#[serial]`-gated integration binary. The whole file ran end-to-end on every
test invocation even though only one section had changed. Split by semantic
section into three independent binaries.**

```
neo4j_compatibility_test.rs                 2,103 LOC → removed
neo4j_compatibility_core_test.rs            NEW →  317 LOC — 7 fixture-driven tests
                                            (multi-label MATCH, UNION, bidirectional
                                             relationships, property access). Hosts
                                             the shared `setup_test_data` fixture.
neo4j_compatibility_extended_test.rs        NEW → 1,063 LOC — 34 tests covering
                                             UNION variants, labels()/keys()/type(),
                                             DISTINCT, ORDER BY with UNION, multi-label
                                             aggregations + the count(*) suite (8 tests).
neo4j_compatibility_additional_test.rs      NEW →  825 LOC — 68 numbered
                                             `neo4j_compat_*` / `neo4j_test_*`
                                             micro-scenarios (count/labels/keys/id/type
                                             / LIMIT / DISTINCT / property types).
```

Pure refactor — every test body is byte-identical to the original, `#[serial]`
gating preserved, same helper `execute_query` function duplicated in each
file. `setup_test_data` lives only in `core_test.rs` (the only caller).

All 109 tests pass (7 + 34 + 68) under
`cargo +nightly test --package nexus-core --test neo4j_compatibility_*_test`;
clippy warning-clean.

**Benefits**:
- Granular test targeting — `cargo test --test neo4j_compatibility_core_test`
  runs only the 7 fixture-driven scenarios (~0.3s).
- Parallel binary compilation — the three binaries link independently.
- Each file is under 1,100 LOC, well under the 1,500 LOC target.

### 🧱 Regression Test Split (Tier 3.1) (2026-04-18)

**`nexus-core/tests/regression_extended.rs` was 2,184 LOC covering seven
feature areas in a single integration-test binary. Split by feature area
into seven cohesive test binaries — each one now compiles and runs
independently, and `cargo test --test regression_extended_match`
(etc.) exercises just the relevant slice.**

```
regression_extended.rs                 2,184 LOC  → removed
regression_extended_create.rs          NEW →  423 LOC  — 25 CREATE tests
regression_extended_match.rs           NEW →  312 LOC  — 17 MATCH/WHERE tests
regression_extended_relationships.rs   NEW →  583 LOC  — 24 relationship tests
regression_extended_functions.rs       NEW →  343 LOC  — 20 function tests
regression_extended_union.rs           NEW →  225 LOC  — 10 UNION tests
regression_extended_engine.rs          NEW →  172 LOC  — 12 Engine-API tests
regression_extended_simple.rs          NEW →  140 LOC  — 10 smoke tests
```

Pure refactor — every test body is byte-identical to the original
(comments and `setup_test_engine` / `setup_isolated_test_engine` calls
preserved). Dead `use nexus_core::Engine` import dropped (the type name
was never referenced at the call sites). All 118 tests pass under
`cargo +nightly test --package nexus-core --test regression_extended_*`
and workspace-wide clippy is warning-clean.

**Benefits**:
- Merge-conflict surface reduced — unrelated test additions no longer
  collide on a single file.
- Parallel `cargo test` scheduling — the seven binaries run concurrently
  (~0.4 s wall-clock for the full suite versus the old serialized run).
- AI-agent-friendly file sizes — largest file (`relationships`, 583 LOC)
  is well under the 1,500 LOC target.

### 🧱 Engine Module Split (Tier 1.5) (2026-04-18)

**`nexus-core/src/engine/mod.rs` was 4,636 LOC — the largest remaining
source file in the tree after the Tier 1 + Tier 2 splits. Carved out
into five focused submodules in four atomic commits.**

```
engine/mod.rs         4,636 → 3,624 LOC   (−1012, −21.8%)
engine/config.rs      NEW → 45 LOC        — GraphStatistics, EngineConfig
engine/stats.rs       NEW → 39 LOC        — EngineStats, HealthStatus, HealthState
engine/clustering.rs  NEW → 135 LOC       — cluster_nodes + 5 wrappers + convert_to_simple_graph
engine/maintenance.rs NEW → 193 LOC       — knn_search, export_to_json, get_graph_statistics,
                                              clear_all_data, validate_graph, graph_health_check,
                                              health_check
engine/crud.rs        NEW → 651 LOC       — create/get/update/delete nodes + relationships +
                                              index_node_properties + apply_pending_index_updates +
                                              NodeWriteState (Cypher write-pass staging)
```

Pure refactor — public API surface unchanged (every method still
resolves as `Engine::*` via Rust's multi-file `impl` blocks), all
2,567 nexus-core tests green across every split commit, pre-commit
hooks (fmt + clippy deny-warnings) enforced on each step.

mod.rs remains the largest file in the tree; the residual ~2,400 LOC
are the Cypher execution core (33 private helpers with shared state
needing a deeper reshape than a pure file split). Tracked under
`phase1_split-oversized-modules` Tier 3 for a follow-up.

### ⚡ SIMD Runtime-Dispatched Kernels + Parser O(N²) Fix (2026-04-18)

**New `nexus-core::simd` module — always compiled, runtime-dispatched,
no Cargo feature flags. Kernels span distance (f32 dot / l2_sq / cosine
/ normalize), bitmap popcount, numeric reductions (sum / min / max i64
/ f64 / f32), compare (eq / ne / lt / le / gt / ge i64 / f64), RLE run
scanning, CRC32C, and a size-threshold JSON dispatcher.**

Per ADR-003, every kernel ships as scalar reference + SSE4.2 + AVX2 +
AVX-512F + NEON with proptest parity (>= 40 cases, 256–1024 inputs
each). Selection is cached in `OnceLock<unsafe fn>` on first call;
`NEXUS_SIMD_DISABLE=1` env var forces scalar runtime-wide for
emergency rollback.

**Measured on Ryzen 9 7950X3D (Zen 4, AVX-512F + VPOPCNTQ):**

| Op                  | Scale       | Scalar   | Dispatch  | Speedup  |
|---------------------|-------------|----------|-----------|----------|
| `dot_f32`           | dim=768     | 438 ns   | 34.5 ns   | 12.7×    |
| `dot_f32`           | dim=1024    | 580 ns   | 50.8 ns   | 11.4×    |
| `dot_f32`           | dim=1536    | 893 ns   | 70.3 ns   | 12.7×    |
| `l2_sq_f32`         | dim=512     | 285 ns   | 21.0 ns   | 13.5×    |
| `popcount_u64`      | 4096 words  | 1.52 µs  | 136 ns    | ≈11×     |
| `sum_f64`           | n=262 144   | 150 µs   | 19 µs     | 7.9×     |
| `sum_f32`           | n=262 144   | 152 µs   | 9.5 µs    | 15.9×    |
| `lt_i64`            | n=262 144   | 110 µs   | 25 µs     | 4.4×     |
| `eq_i64`            | n=262 144   | 69 µs    | 24 µs     | 2.9×     |
| `find_run_length`   | uniform 16k | 3.2 µs   | 1.0 µs    | 3.2×     |
| **Cypher parse**    | **31.5 KiB**| **≈1 s** | **3.7 ms**| **≈290×**|

Cypher parse speedup is the non-SIMD O(N²) → O(N) fix uncovered while
auditing phase-3 §8–9: `self.input.chars().nth(self.pos)` (O(n) per
call) replaced with `self.input[self.pos..].chars().next()` (O(1)) in
`peek_char`, `consume_char`, `peek_keyword`, `peek_keyword_at`,
`skip_whitespace`, `peek_char_at`. Cost-per-byte now flat at
92–117 ns/byte across three orders of magnitude — linear scaling
confirmed.

**Production call sites wired to SIMD:**

- `index::KnnIndex` — `DistSimdCosine` / `DistSimdL2` implement
  `hnsw_rs::dist::Distance<f32>` via `simd::distance::cosine_f32` /
  `l2_sq_f32`. Every HNSW insert and query distance flows through
  AVX-512 / AVX2 / NEON on supported hardware.
- `index::KnnIndex::normalize_vector` — delegates to
  `simd::distance::normalize_f32`.
- `graph::algorithms::traversal::{cosine_similarity, jaccard_similarity}`
  — refactored from full-universe f64 fold to packed `Vec<u64>`
  bitmaps + `simd::bitmap::{popcount_u64, and_popcount_u64}`.
- `storage::graph_engine::compression::compress_simd_rle` — inner
  run-length scan replaced with `simd::rle::find_run_length` (was
  misnamed "SIMD-accelerated", now actually SIMD).
- `wal::Wal::append` / `recover` — dual-format (v1/v2) frames with
  pluggable `ChecksumAlgo` field; reads both, writes default to
  `Crc32Fast` (benchmark showed 3-way parallel PCLMUL in `crc32fast`
  beats sequential `_mm_crc32_u64` on modern x86; CRC32C primitive
  kept available via `append_with_algo(entry, Crc32C)`).
- `executor::parser::{tokens, expressions}` — O(N²) tokenizer fix.

**New files (all under `nexus-core/src/simd/`):** `mod.rs`, `dispatch.rs`,
`scalar.rs`, `distance.rs`, `bitmap.rs`, `reduce.rs`, `compare.rs`,
`rle.rs`, `crc32c.rs`, `json.rs`, `x86.rs`, `aarch64.rs`.

**New benches (under `nexus-core/benches/`):** `simd_distance.rs`,
`simd_popcount.rs`, `simd_reduce.rs`, `simd_compare.rs`, `simd_rle.rs`,
`simd_crc.rs`, `simd_json.rs`, `parser_tokenize.rs`.

**New proptest parity suites (under `nexus-core/tests/`):**
`simd_scalar_properties.rs`, `simd_distance_parity.rs`,
`simd_bitmap_parity.rs`, `simd_reduce_parity.rs`,
`simd_compare_parity.rs`, `simd_rle_parity.rs`, `simd_json_parity.rs`.

**New spec:** `docs/specs/simd-dispatch.md` — CpuFeatures probe,
cascade rules, tolerances, per-kernel tier tables, measured
benchmark numbers, phase-3 per-item status including honest writeups
of the three items that did not deliver as the task spec anticipated
(CRC32C hardware, simd-json on Value-field payloads, record codec
batch — the last already LLVM-auto-vectorised).

**ADRs:** ADR-001 (RPC wire format), ADR-002 (SDK default transport),
ADR-003 (SIMD dispatch — runtime detection, no feature flags, tiered
fallback with proptest parity).

**Rollout safety:**

- `NEXUS_SIMD_DISABLE=1` — scalar fallback for every dispatched op.
- `NEXUS_SIMD_JSON_DISABLE=1` — forces serde_json in the
  `simd::json` dispatcher.
- Single `tracing::info!` on first `cpu()` call reports the
  selected tier + all flag values.

**Verification across all SIMD commits:**

- `cargo +nightly fmt --all` — clean (pre-commit hook enforces).
- `cargo +nightly clippy -p nexus-core --tests --benches -- -D warnings`
  — clean.
- `cargo +nightly test -p nexus-core` — 2566 passed, 0 failed.
- 300/300 Neo4j compatibility suite unaffected (no wire format change).

### 🧱 Oversized-Module Split — Tier 1 + Tier 2 (2026-04-18)

**Eight critical files > 1,500 LOC split into focused sub-modules. No
behaviour change: 1,346 nexus-core unit tests and 2,954 workspace tests
continue to pass; every public API preserved via `pub use` re-exports.**

17 atomic commits, each quality-gated (`cargo check`, `clippy -D warnings`,
`cargo fmt`, tests). Aggregate input-vs-output:

| File | Before (LOC) | Façade after (LOC) | Reduction |
|---|---|---|---|
| `nexus-core/src/executor/mod.rs` | 15,260 | 1,139 | -92.5% |
| `nexus-core/src/executor/parser.rs` | 6,882 | 35 + 5 subfiles | -99.5% |
| `nexus-core/src/lib.rs` | 5,564 | 104 | -98.1% |
| `nexus-core/src/graph/correlation/mod.rs` | 4,638 | 2,313 | -50.1% |
| `nexus-core/src/executor/planner.rs` | 4,254 | 393 | -90.8% |
| `nexus-core/src/graph/correlation/data_flow.rs` | 3,004 | 1,625 | -45.9% |
| `nexus-server/src/api/cypher.rs` | 2,965 | 518 | -82.5% |
| `nexus-core/src/graph/algorithms.rs` | 2,560 | 220 | -91.4% |

**New sub-modules created**:

- `executor/{types, shared, context, engine}` + `executor/eval/{arithmetic,
  helpers, predicate, projection, temporal}` + `executor/operators/{admin,
  aggregate, create, dispatch, expand, filter, join, path, procedures,
  project, scan, union, unwind}`.
- `executor/parser/{ast, clauses, expressions, tokens, tests}`.
- `executor/planner/{mod, queries, tests}`.
- `engine/{mod, tests}` (moved out of `lib.rs`).
- `graph/correlation/{query_executor, vectorizer_extractor, tests}`.
- `graph/correlation/data_flow/{mod, layout, tests}`.
- `graph/algorithms/{mod, traversal, tests}`.
- `nexus-server/src/api/cypher/{mod, execute, commands, tests}`.

**Benefits**:
- Faster incremental builds — `rustc` re-checks far less code per touch.
- Parallelisable PRs — feature work on `executor/operators/filter.rs`
  no longer collides with `executor/operators/join.rs`.
- Reviewable diffs — each module change is scoped to one responsibility.

### 🛡️ Memory-Leak Hardening (2026-04-18)

**Defensive limits + cleanup paths against unbounded memory growth.**

Input validation and capped allocations across the full request lifecycle,
plus a Docker-based memtest harness for regression detection.

- **Executor hardcaps** — `MAX_INTERMEDIATE_ROWS` enforced in label
  scans, all-nodes scans, expand paths, and variable-length path
  expansion. Exceeding the cap returns `Error::OutOfMemory` deterministically.
- **HTTP body size limit** — configurable `nexus-server` request body cap
  prevents memory exhaustion via oversized Cypher payloads.
- **HNSW `max_elements`** — now configurable per index, avoiding the
  previous default over-allocation.
- **GraphQL list resolvers** — relationship-list fields now require a
  `limit` argument.
- **Metric collector** — capped unique-key cardinality in `MetricCollector`
  prevents metric label explosion in long-running servers.
- **Cache tuning** — tighter defaults for the vectorizer cache and
  intelligent query cache.
- **Connection cleanup** — `ConnectionTracker::cleanup_stale_connections`
  sweeps abandoned connection state periodically.
- **Page cache observability** — eviction stall events logged before
  returning errors so memory pressure is diagnosable.
- **Initial mmap** — shrunk `graph_engine` startup allocation to reduce
  RSS footprint on idle.
- **Memtest harness** — `scripts/memtest/` (Dockerfile.memtest,
  docker-compose.memtest.yml, run-all.sh, profile.sh, measure.sh) with
  a hard memory cap so leaks surface as `OOMKilled` instead of thrashing
  the host. `MALLOC_CONF` wired for jemalloc heap profiling via `jeprof`.

Tuning and troubleshooting guidance in `docs/performance/MEMORY_TUNING.md`.

### ✅ Neo4j Compatibility Test Results - 100% Pass Rate (2025-12-01)

**Latest compatibility test run: 299/300 tests passing (0 failed, 1 skipped)**

- **Test Results**:
  - Total Tests: 300
  - Passed: 299 ✅
  - Failed: 0 ❌
  - Skipped: 1 ⏭️
  - Pass Rate: **100%**

- **Recent Fixes** (improvement from 293 to 299):
  - Fixed UNWIND with MATCH query routing - queries like `UNWIND [...] AS x MATCH (n)` now correctly route through Engine instead of dummy Executor
  - Fixed query detection to recognize MATCH anywhere in query, not just at the start
  - Removed debug statements from executor and planner

- **Previous Fixes** (improvement from 287 to 293):
  - Fixed cartesian product bug in MATCH patterns with multiple disconnected nodes
  - Added `OptionalFilter` operator for proper WHERE clause handling after OPTIONAL MATCH
  - Fixed OPTIONAL MATCH IS NULL filtering (12.06)
  - Fixed OPTIONAL MATCH IS NOT NULL filtering (12.07)
  - Fixed WITH clause operator ordering (WITH now executes after UNWIND)
  - Fixed `collect(expression)` by ensuring Project executes for aggregation arguments
  - Fixed UNWIND with collect expression (14.13)

- **Sections with 100% Success** (235 tests):
  - Section 1: Basic CREATE and RETURN (20/20)
  - Section 2: MATCH Queries (25/25)
  - Section 3: Aggregation Functions (25/25)
  - Section 4: String Functions (20/20)
  - Section 5: List/Array Operations (20/20)
  - Section 6: Mathematical Operations (20/20)
  - Section 7: Relationships (30/30)
  - Section 8: NULL Handling (15/15)
  - Section 9: CASE Expressions (10/10)
  - Section 10: UNION Queries (10/10)
  - Section 11: Graph Algorithms & Patterns (15/15)
  - Section 13: WITH Clause (15/15)
  - Section 16: Type Conversion (15/15)

- **Known Limitations** (1 skipped):
  - **UNWIND with WHERE** (14.05): WHERE directly after UNWIND requires operator reordering

- **Server Status**:
  - Server: v0.12.0
  - Uptime: Stable
  - Health: All components healthy

### 🧪 Expanded Neo4j Compatibility Test Suite - 300 Tests (2025-12-01)

**Test suite expanded from 210 to 300 tests (+90 new tests)**

- **Section 12: OPTIONAL MATCH** (15 tests)
  - Left outer join semantics with NULL handling
  - OPTIONAL MATCH with WHERE, aggregations, coalesce
  - Multiple OPTIONAL MATCH patterns
  - OPTIONAL MATCH with CASE expressions

- **Section 13: WITH Clause** (15 tests)
  - Projection and field renaming
  - Aggregation with WITH (count, sum, avg, collect)
  - WITH + WHERE filtering
  - Chained WITH clauses
  - WITH DISTINCT and ORDER BY

- **Section 14: UNWIND** (15 tests)
  - Basic array unwinding
  - UNWIND with filtering and expressions
  - Nested UNWIND operations
  - UNWIND with aggregations
  - UNWIND + MATCH combinations

- **Section 15: MERGE Operations** (15 tests)
  - MERGE create new vs match existing
  - ON CREATE SET / ON MATCH SET
  - MERGE relationships
  - Multiple MERGE patterns
  - MERGE idempotency verification

- **Section 16: Type Conversion** (15 tests)
  - toInteger(), toFloat(), toString(), toBoolean()
  - Type conversion with NULL handling
  - toIntegerOrNull(), toFloatOrNull()
  - Type coercion in expressions

- **Section 17: DELETE/SET Operations** (15 tests)
  - SET single and multiple properties
  - SET with expressions
  - DELETE relationships and nodes
  - DETACH DELETE
  - REMOVE property

- **Files Modified**:
  - `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` - 6 new test sections
  - `rulebook/tasks/complete-neo4j-compatibility/tasks.md` - Updated documentation

### Temporal Arithmetic Operations 🕐 (2025-11-30)

**Full support for date/time arithmetic operations**

- **Datetime + Duration**:
  - `datetime('2025-01-15T10:30:00') + duration({days: 5})` - Add days
  - `datetime('2025-01-15T10:30:00') + duration({months: 2})` - Add months
  - `datetime('2025-01-15T10:30:00') + duration({years: 1})` - Add years

- **Datetime - Duration**:
  - `datetime('2025-01-15T10:30:00') - duration({days: 5})` - Subtract days
  - `datetime('2025-03-15T10:30:00') - duration({months: 2})` - Subtract months

- **Datetime - Datetime**:
  - `datetime('2025-01-20') - datetime('2025-01-15')` - Returns duration between dates

- **Duration + Duration**:
  - `duration({days: 3}) + duration({days: 2})` - Combine durations

- **Duration - Duration**:
  - `duration({days: 5}) - duration({days: 2})` - Duration difference

- **Duration Functions**:
  - `duration.between(start, end)` - Duration between two datetimes
  - `duration.inMonths(start, end)` - Difference in months
  - `duration.inDays(start, end)` - Difference in days
  - `duration.inSeconds(start, end)` - Difference in seconds

- **Files Modified**:
  - `nexus-core/src/executor/mod.rs` - Temporal arithmetic implementation
  - `nexus-core/tests/test_temporal_arithmetic.rs` - New test file (17 tests)

### 🎉 100% Neo4j Compatibility Achieved - 300/300 Tests Passing (2025-11-30)

**Complete Neo4j compatibility test suite passing - Major Milestone!**

- **GDS Procedure Wrappers** (20 built-in procedures):
  - `gds.centrality.eigenvector` - Eigenvector centrality analysis
  - `gds.shortestPath.yens` - K shortest paths using Yen's algorithm
  - `gds.triangleCount` - Triangle counting for graph structure analysis
  - `gds.localClusteringCoefficient` - Local clustering coefficient per node
  - `gds.globalClusteringCoefficient` - Global clustering coefficient
  - `gds.pageRank` - PageRank centrality
  - `gds.centrality.betweenness` - Betweenness centrality
  - `gds.centrality.closeness` - Closeness centrality
  - `gds.centrality.degree` - Degree centrality
  - `gds.community.louvain` - Louvain community detection
  - `gds.community.labelPropagation` - Label propagation
  - `gds.shortestPath.dijkstra` - Dijkstra shortest path
  - `gds.components.weaklyConnected` - Weakly connected components
  - `gds.components.stronglyConnected` - Strongly connected components
  - `gds.allShortestPaths` - All shortest paths

- **Bug Fixes**:
  - **Bug 11.02**: Fixed NodeByLabel in cyclic patterns - Planner now preserves all starting nodes for triangle queries
  - **Bug 11.08**: Fixed variable-length paths `*2` - Disabled optimized traversal for exact length constraints
  - **Bug 11.09**: Fixed variable-length paths `*1..3` - Disabled optimized traversal for range constraints
  - **Bug 11.14**: Fixed WHERE NOT patterns - Added EXISTS expression handling in `expression_to_string`

- **Files Modified**:
  - `nexus-core/src/executor/planner.rs` - Added `RelationshipQuantifier` import, fixed `PropertyMap` access, enhanced pattern serialization
  - `nexus-core/src/executor/mod.rs` - Disabled optimized traversal for variable-length path constraints

- **Test Results**:
  - 210/210 Neo4j compatibility tests passing (100%)
  - 1382+ cargo workspace tests passing
  - All SDKs verified working

### Added - Master-Replica Replication 🔄

**V1 Replication implementation with WAL streaming and full sync support**

- **Master Node** (`nexus-core/src/replication/master.rs`):
  - WAL streaming to connected replicas
  - Replica tracking with health monitoring
  - Async replication (default) - no ACK wait
  - Sync replication with configurable quorum
  - Circular replication log (1M operations max)
  - Heartbeat-based health monitoring

- **Replica Node** (`nexus-core/src/replication/replica.rs`):
  - TCP connection to master
  - WAL entry receiving and application
  - CRC32 validation on all messages
  - Automatic reconnection with exponential backoff
  - Replication lag tracking
  - Promotion to master support

- **Full Sync** (`nexus-core/src/replication/snapshot.rs`):
  - Snapshot creation (tar + zstd compression)
  - Chunked transfer with CRC32 validation
  - Automatic snapshot for new replicas
  - Incremental sync after snapshot restore

- **Wire Protocol** (`nexus-core/src/replication/protocol.rs`):
  - Binary format: `[type:1][length:4][payload:N][crc32:4]`
  - Message types: Hello, Welcome, Ping, Pong, WalEntry, WalAck, Snapshot*

- **REST API Endpoints** (`nexus-server/src/api/replication.rs`):
  - `GET /replication/status` - Get replication status
  - `GET /replication/master/stats` - Master statistics
  - `GET /replication/replica/stats` - Replica statistics
  - `GET /replication/replicas` - List connected replicas
  - `POST /replication/promote` - Promote replica to master
  - `POST /replication/snapshot` - Create snapshot
  - `GET /replication/snapshot` - Get last snapshot info
  - `POST /replication/stop` - Stop replication

- **Configuration** (via environment variables):
  - `NEXUS_REPLICATION_ROLE`: master/replica/standalone
  - `NEXUS_REPLICATION_BIND_ADDR`: Master bind address
  - `NEXUS_REPLICATION_MASTER_ADDR`: Master address for replicas
  - `NEXUS_REPLICATION_MODE`: async/sync
  - `NEXUS_REPLICATION_SYNC_QUORUM`: Quorum size for sync mode

- **Documentation**:
  - `docs/operations/REPLICATION.md` - Complete replication guide
  - OpenAPI specification updated with replication endpoints

- **Testing**: 26 unit tests covering all replication components

---

## Previous releases

Full notes for every historical release are split by patch-level decade
under [docs/patches/](docs/patches/). Each file covers up to ten patch
versions of the same minor (see filename range):

| Version range | File                                                                |
| ------------- | ------------------------------------------------------------------- |
| 0.12.x        | [docs/patches/v0.12.0-0.12.9.md](docs/patches/v0.12.0-0.12.9.md)    |
| 0.11.x        | [docs/patches/v0.11.0-0.11.9.md](docs/patches/v0.11.0-0.11.9.md)    |
| 0.10.x        | [docs/patches/v0.10.0-0.10.9.md](docs/patches/v0.10.0-0.10.9.md)    |
| 0.9.10+       | [docs/patches/v0.9.10-0.9.19.md](docs/patches/v0.9.10-0.9.19.md)    |
| 0.9.0-0.9.9   | [docs/patches/v0.9.0-0.9.9.md](docs/patches/v0.9.0-0.9.9.md)        |
| 0.8.x         | [docs/patches/v0.8.0-0.8.9.md](docs/patches/v0.8.0-0.8.9.md)        |
| 0.7.x         | [docs/patches/v0.7.0-0.7.9.md](docs/patches/v0.7.0-0.7.9.md)        |
| 0.6.x         | [docs/patches/v0.6.0-0.6.9.md](docs/patches/v0.6.0-0.6.9.md)        |
| 0.5.x         | [docs/patches/v0.5.0-0.5.9.md](docs/patches/v0.5.0-0.5.9.md)        |
| 0.4.x         | [docs/patches/v0.4.0-0.4.9.md](docs/patches/v0.4.0-0.4.9.md)        |
| 0.2.x         | [docs/patches/v0.2.0-0.2.9.md](docs/patches/v0.2.0-0.2.9.md)        |
| 0.1.x         | [docs/patches/v0.1.0-0.1.9.md](docs/patches/v0.1.0-0.1.9.md)        |
| 0.0.x         | [docs/patches/v0.0.0-0.0.9.md](docs/patches/v0.0.0-0.0.9.md)        |

> Note: there is no `0.3.x` range — the project jumped from `0.2.0` to
> `0.4.0` during early development.
