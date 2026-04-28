# 09 — Distributed (V2) Cluster Mode Status

Sources: `crates/nexus-core/src/{sharding,coordinator,cluster}/`, `docs/CLUSTER_MODE.md`, `docs/specs/cluster-mode.md`, CHANGELOG.md (V2 entries), `.rulebook/tasks/phase5_implement-cluster-mode/`.

## Phase status

| Phase | Status | Tests |
|-------|--------|-------|
| 0 — scaffolding | ✅ done | — |
| 1 — single-node MVP | ✅ done (storage, indexes, executor, API, SDKs) | 133 |
| 2 — V1 hardening (Neo4j compat 100 %, replication v1, auth, GDS, correlation) | ✅ done | — |
| 3 — V2 sharded core | ✅ done 2026-04-20, commit `7701ae6e` | **+201 V2-dedicated** |
| 5 — cluster mode multi-tenancy | 🚧 ongoing — 93/125 items (74 %) | 3470+ workspace |
| 6 — spatial planner + open-cypher follow-ups | 🚧 in flight (current branch) | landed v1.2.0 |
| Future — automatic failover, multi-region, GNN, temporal, streaming, OLAP | 📋 planned V2.1+ | — |

## What V2 ships (end-to-end)

### Sharding (`crates/nexus-core/src/sharding/`, 143 unit tests)

- **Hash-based assignment** via xxh3 on `node_id` — deterministic across restarts.
- **Relationships live on the source node's shard** with optional remote-anchor records for reverse traversal.
- **Iterative rebalance plan** (`rebalance.rs`) — generation-aware, deterministic, converges over multiple rounds.
- **Generation-tagged metadata** (`metadata.rs`) — every cluster-state change bumps a `u64` generation; stale lookups detected via `ERR_STALE_GEN`.
- **Shard count configurable** via `[cluster.sharding]` config; opt-in (standalone deployments unaffected).
- **Health monitoring** (`health.rs`) — per-shard + per-replica, majority-rule + TTL semantics.

### Per-shard Raft consensus (`crates/nexus-core/src/sharding/raft/`, 65 unit tests)

- **Bespoke Raft impl** (not openraft — that's still 0.10-alpha).
- **Per-shard scope** — each shard replicates its log across `replica_factor` nodes via its own Raft group.
- **Global metadata Raft group** — separate, replicated across all cluster members; manages shard assignments + node membership + generation numbers.
- **§5.4.2 commit-from-current-term** enforced — only entries from the current term can commit (prevents stale-leader log overwrites).
- **§5.3 truncate-on-conflict** semantics — followers reject mismatched entries.
- **Snapshot install** — reuses v1 zstd+tar `replication::Snapshot` format.
- **Wire format** `[shard_id:u32][msg_type:u8][len:u32][payload:N][crc32:u32]`.
- **Single-writer invariant** — only the Raft leader's apply loop touches a shard's storage layer.
- **Tested scenarios:** 3-node failover, 5-node partition tolerance (majority continues), single-node bootstrap, follower catch-up from snapshot.

### Distributed query coordinator (`crates/nexus-core/src/coordinator/`, 46 unit tests)

1. **Query classification** (`classify.rs`) — Scope: `SingleShard` / `Targeted` / `Broadcast`.
2. **Plan decomposition** (`plan.rs`) — splits a logical plan into shard-local subplans + a coordinator merge op. Pushdown-ready (no second round-trip).
3. **Cross-shard traversal** (`cross_shard.rs`) — LRU cache (10 K entries, TTL) for remote-node fetches; budget-aware (default 2-hop limit).
4. **Scatter-gather runtime** (`scatter.rs`) — parallel subplan dispatch to shard leaders; **atomic per-query failure** (any shard down → whole query fails; no partial rows). Leader-hint retry up to 3×; stale-generation refresh 1 pass.
5. **Merge operators** (`merge.rs`) — AggregationMerge (COUNT / SUM / AVG / MIN / MAX / COLLECT), SortKey for coordinator-level ORDER BY. Neo4j NULL semantics.
6. **TCP transport** (`tcp_client.rs`) — `TcpShardClient` with leader cache + cache invalidation on `ERR_STALE_GEN`. Generic `ShardClient` trait — production uses TCP, tests use `InMemoryShardClient`.

### Cluster management API (`crates/nexus-server/src/api/cluster.rs`)

- `GET /cluster/status` — cluster state, shard assignments, generations.
- `POST /cluster/add_node` — add a node.
- `POST /cluster/remove_node` — remove a node (drains shards first).
- `POST /cluster/rebalance` — trigger rebalance.
- `GET /cluster/shards/{id}` — health + replica status of a single shard.
- **Authorization:** admin-gated via existing RBAC; no new permission model.
- **Write failure handling:** `307 Temporary Redirect` on follower write attempts — clients retry on leader.

### Multi-host TCP transport (2026-04-20)

- Raft replicas talk over TCP across hosts.
- Single-host in-process transport retained for tests.

### Cluster mode multi-tenancy (Phase 5, 74 % complete)

- **AST-rewrite catalog-prefix isolation** — every tenant has a logical namespace; queries are rewritten before execution to scope to that namespace. Tested end-to-end against a two-tenant attack surface.
- **Quota + rate-limiting**: `LocalQuotaProvider` (drop-in for the upcoming HiveHub SDK).
- **ADR-7** documents the design choice (AST-rewrite vs per-tenant storage prefix; chose AST-rewrite to keep storage layer unchanged).
- **Phase 1 (HiveHub SDK integration)** — blocked on external `hivehub-cloud-internal-sdk` crate not yet published.
- **Phase 2 §5 (direct storage-prefix namespacing)** — superseded by §6 (AST-rewrite).

## Replication (v1, pre-V2)

- **Master-replica** WAL streaming, async by default; sync optional (master waits for quorum ack).
- **Manual promotion** via `POST /replication/promote` — automatic failover deferred to V2.1.
- **Heartbeat 5 s** (configurable `NEXUS_REPLICATION_HEARTBEAT_MS`); `replication_lag` exposed at `/replication/replica/stats`.
- **Auto-reconnect** with exponential backoff.

In V2, **per-shard Raft replaces the master-replica model for shard data** — so v1 replication is now a single-node-with-read-replicas option, while V2 cluster mode is the multi-node story. Both ship.

## What's NOT in V2

| Gap | Severity | Notes |
|-----|----------|-------|
| **Cross-shard 2PC / multi-shard write transactions** | **Critical** | V2 supports single-shard writes only. Multi-shard mutations fail-atomic but no cross-shard ACID. |
| **Read-consistency contract across shards mid-query** | **High** | Coordinator re-validates shard generation, but a latency window exists where shard A may be at gen N and shard B at gen N+1. |
| **Online re-sharding** | High | `RebalancePlan` exists; not exercised end-to-end on running clusters under load. |
| **Automatic failover for v1 master-replica path** | Medium | Manual via `/replication/promote`. V2.1. |
| **Multi-region / geo-replication** | Medium | V2.1+. |
| **Backup coordination across shards** | Medium | Per-node manual. |
| **Snapshot install latency SLO** | Low | Not documented. |
| **Coordinator timeout-tuning guide per workload** | Low | Mentioned, not detailed. |
| **Metadata-group availability story** | Low | If metadata Raft is down, shard health changes can't propagate — not explicitly addressed. |

## Failure modes — documented vs unhandled

**Documented:**
- Generation staleness (`ERR_STALE_GEN`) — client retries.
- Leader-hint retry (3 attempts, then error).
- Cross-shard traversal budget limits (default 2-hop, prevents runaway).
- Atomic query failure on shard outage.

**Unhandled / open:**
- Network partitions — Raft tolerates minority partitions (they can't commit), but client routing under partition is unspecified.
- Snapshot recovery time SLO — none.
- Tail-latency under shard-leader churn — not benchmarked.
- Coordinator memory pressure under high-fanout broadcast queries — not benchmarked.

## Test coverage

| Component | Path | Unit | Integration |
|-----------|------|------|-------------|
| Sharding | `crates/nexus-core/src/sharding/` | 143 | — |
| Raft | `crates/nexus-core/src/sharding/raft/` | 65 | — |
| Coordinator | `crates/nexus-core/src/coordinator/` | 46 | — |
| Cluster isolation | `crates/nexus-core/tests/cluster_isolation_tests.rs` | — | ~20 |
| V2 E2E | `crates/nexus-core/tests/v2_sharding_e2e.rs` | — | ~12 |
| TCP cluster | `crates/nexus-core/tests/v2_tcp_cluster_integration.rs` | — | (count not surfaced) |
| Cluster mode (multi-tenant) | `crates/nexus-core/src/cluster/` | ~37 | 4 |
| **Total V2-dedicated** | — | — | **~201** |
| Workspace global | — | — | **3470+ passing** |

## Recommendations

1. **Cross-shard 2PC or pessimistic per-shard locking** — without this, V2 is "single-shard writes only," which is the structural gating issue vs Memgraph HA / Arango cluster / Dgraph distributed-first. **Effort: 4–6 weeks.** Highest priority.
2. **Read-consistency contract across shards** — define and document the semantics; tests should pin them down. ~2 weeks.
3. **Online re-sharding under load** — integration test that exercises rebalance during sustained read+write. ~2 weeks.
4. **Chaos / failure injection** — partition shards, drop leader, slow disk, clock skew. ~1 week to scaffold + ongoing.
5. **Cluster perf benchmarks** — Raft consensus latency, cross-shard query, replication overhead, vs single-node. ~1 week.
6. **Metadata-group availability** — what happens when the global metadata Raft is down? Document + test. ~3 days.
7. **Automatic failover for v1 replica path** — even minimal heartbeat-based promotion. ~2 weeks.
8. **Snapshot recovery SLO + observability** — surface `snapshot_install_seconds` metric. ~1 week.
9. **Document the V2 production-readiness gate** — currently V2 is "single-shard writes safe, multi-shard writes unsupported." This should be in big bold text in `CLUSTER_MODE.md`.

## Bottom line

V2 cluster core is **functionally complete for sharded reads + single-shard writes** (201 dedicated tests, all passing). The piece that gates calling V2 "production cluster mode" rather than "production single-node + read-fanout" is **cross-shard 2PC**. Once that lands plus chaos-test coverage, V2 is ready to advertise as a Memgraph-HA / Arango-cluster competitor; until then it's a Kuzu-vacancy / "scale reads, partition writes" story.
