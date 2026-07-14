# Performance Assessment & Optimization Roadmap

> **Date**: 2026-07-11 · **Analyzed version**: 2.4.0 · **Method**: code review
> of executor/planner/page-cache/WAL/locking + audit of existing benchmark
> reports (`docs/performance/`).
>
> Part of the [Nexus 2.5.0 competitive analysis](README.md).

## Current architecture (execution model)

- **Two parallel execution paths with unequal concurrency**: `Arc<Executor>`
  (lock-free, clonable per query) vs `Arc<TokioRwLock<Engine>>` (monolithic,
  `&mut self` required). **Every MATCH/CREATE/MERGE/DDL/transaction query
  takes the full engine write lock** — only bare `RETURN`/`WITH`/`UNWIND`
  without a pattern use the lock-free path.
- **Row-at-a-time (Volcano) interpretation over `serde_json::Value`** rows.
  A columnar fast path exists but re-materializes typed columns from JSON
  values on every call — measured at only 1.13x (filter) and 0.92–1.03x
  (aggregates, sometimes *slower*).
- **Heuristic cost model with fixed constants** (Expand=100, Join=200,
  VarLenPath=500); only label selectivity is live. A `StatisticsCollector`
  exists but is not wired into join ordering.
- **Storage**: fixed-size records (32B node / 48B rel), doubly-linked
  adjacency lists (O(1) hop), LMDB catalog, mmap record stores.
- **Page cache**: Clock (second-chance) only — 2Q/TinyLFU mentioned in docs
  are NOT implemented; pages are memcpy'd out of the mmap.
- **Transactions**: global single-writer mutex; epoch MVCC for readers.
- **WAL**: synchronous `sync_all()` per commit by default; `AsyncWalWriter`
  exists as opt-in.
- **SIMD**: real and runtime-dispatched (AVX-512→AVX2→SSE4.2→NEON→scalar);
  honest 12–16x kernel-level wins — which don't reach query level (see #2).
- **No Bolt protocol**: REST/JSON + custom binary RPC + RESP3.

## Measured numbers (existing reports — with caveats)

| Metric | Number | Source / caveat |
|---|---|---|
| Overall throughput (post phase9) | Nexus 603.9 qps vs Neo4j 525.0 (+15%) | `BENCHMARK_RESULTS_PHASE9.md`; win applies mainly to the narrow lock-free path |
| Rel traversal single-hop | **41–57% slower** than Neo4j | persistent across benchmark generations |
| COUNT / GROUP BY | **44.7% / 39.3% slower** | phase9 |
| COLLECT | 34.4% faster | phase9 |
| CREATE node | 78.9% faster | phase9 |
| CREATE relationship | **87.6% slower** (phase9) vs **42.7x faster** (Dec-2025 doc, self-flagged stale) | **contradictory — must re-baseline before trusting either** |
| Columnar vs row executor | 1.13x filter; 0.92–1.03x aggregate | `PERFORMANCE_V1.md` — materialization cost eats SIMD gains |
| SIMD kernels (isolated) | dot_f32 12.7x, l2 13.5x, popcount 11x | kernel-only |
| KNN recall/latency (SIFT1M/GloVe) | **not measured** | `KNN_RECALL.md` is methodology-only; the "<2ms p95 / 10K qps" claims are unverified |

## Bottleneck ranking (highest leverage first)

| # | Bottleneck | Why it costs | Expected gain | Effort |
|---|---|---|---|---|
| 1 | **Global engine write-lock gates ~all real Cypher** (`engine.write().await` around parse+plan+execute for MATCH/CREATE/MERGE/DDL/tx) | Serializes reads against reads server-wide; CPU pinned to ~1 core; the lock-free `Arc<Executor>` path already exists but only synthetic queries reach it | The same fix class already produced a 3.7x jump (162→603 qps) on the narrow path; applying to the dominant MATCH path ≥ that under concurrency | **M** |
| 2 | **Row-at-a-time over `serde_json::Value`** | Materialization from JSON kills the columnar path (measured 0.92–1.13x vs 4–8x kernel potential) | Unlocks the SIMD investment at query level | **L** (foundational) |
| 3 | **Fixed-constant cost model** | JOIN-shaped queries 43–61% slower; nested aggregation 50–60% slower — persistent gap | 30–55% on complex queries | **M** (StatisticsCollector exists, needs wiring) |
| 4 | **Relationship traversal** — no type pre-filter before linked-list walk; `AdvancedTraversalEngine` inconsistently used | Every hop touches full rel records regardless of type selectivity | 30–50% on traversal | **M** |
| 5 | **Aggregation** — no COUNT(*) metadata shortcut (catalog already tracks per-label counts); unsized GROUP BY HashMap | Full materialization for answers the catalog already knows | COUNT(*) → ~O(1); GROUP BY 40–60% | **S** (COUNT) / **M** (GROUP BY) |
| 6 | **Page cache Clock-only + memcpy per fault** | Clock underperforms 2Q/W-TinyLFU on scan-heavy skewed graph access | 10–20% on miss-heavy workloads | **M** |
| 7 | **Sync per-commit WAL fsync** + contradictory write benchmarks | fsync caps write throughput at disk latency; CREATE-rel numbers are contradictory — baseline untrustworthy | 50–70% write-heavy (after re-baseline) | **S–M** |
| 8 | **No Bolt / ~1ms HTTP+JSON floor per query** | Fixed ~0.85–1.35ms floor even on trivial ops; ecosystem lock-out | Order-of-magnitude lower floor for driver clients; ecosystem win | **L** |

## Competitive positioning

- **vs Neo4j**: same storage philosophy (records + linked adjacency), but the
  global engine lock negates the concurrency payoff Neo4j delivers; planner
  maturity is the second gap. Wins: node writes, COLLECT, native KNN.
- **vs Memgraph**: Memgraph has genuinely concurrent MVCC readers and speaks
  Bolt (inheriting the whole Neo4j ecosystem); Nexus must fix #1 and ship
  Bolt to compete head-on.
- **vs Kùzu**: Kùzu's native columnar storage + whole-query vectorization
  dominates analytical workloads; Nexus has the kernels but not the
  representation (#2).
- **vs FalkorDB**: GraphBLAS matrix traversal wins bulk multi-hop/algorithmic
  workloads; Nexus is competitive on shallow 1–2 hop patterns only.
- **Differentiator**: per-label HNSW integrated in the storage layer is
  architecturally tighter than Neo4j's bolt-on or FalkorDB's RediSearch — but
  zero published recall@k curves; publishing SIFT1M/GloVe numbers is required
  for the claim to count.

## 2.5.0 performance scope (chosen)

1. **Kill the global lock for reads** (#1) — route MATCH-only queries through
   the lock-free executor path; writes keep single-writer ordering. Gate:
   before/after concurrency bench.
2. **COUNT(*) metadata shortcut** (#5-S) — cheap, visible, closes a headline
   benchmark gap.
3. **Re-baseline the benchmark suite** (#7 precondition) — resolve the
   CREATE-rel contradiction; publish one trustworthy vs-Neo4j report.
4. **Relationship type pre-filter** (#4) — biggest traversal lever.
5. **Statistics-driven join ordering** (#3) — start with wiring the existing
   StatisticsCollector.

Deferred post-2.5.0: columnar representation (#2), page-cache policy (#6),
group-commit WAL (#7), Bolt (#8 — tracked as its own epic in
[05-v2.5.0-plan.md](05-v2.5.0-plan.md)).

## Bottleneck #1 — implementation status (phase5_lock-free-read-path)

Landed. Autocommit read-only queries (`MATCH` / `OPTIONAL MATCH` / `WITH` /
`UNWIND` / `UNION` / ... with no write clause, no DDL, not inside an open
explicit transaction — see `routing::is_read_only` in
`crates/nexus-server/src/api/cypher/routing.rs`) now run through a cloned
`Engine::executor` snapshot inside `tokio::task::spawn_blocking`, taking only
a brief **shared** `engine.read().await` to obtain the clone and check the
"default" session's transaction state, instead of the exclusive
`engine.write().await` every MATCH previously held for its entire
parse+plan+execute duration. Writes and in-transaction reads are unchanged —
both still take the exclusive engine lock (single-writer ordering and
read-your-own-writes are preserved; see the routing module and the
`lock_free_read_path_test.rs` integration tests for the correctness
argument). The RPC `CYPHER` dispatcher (`protocol/rpc/dispatch/cypher.rs`)
got the identical routing change.

**Measured** (`crates/nexus-server/tests/lock_free_read_path_test.rs`,
`concurrent_match_throughput`, release build, 8 concurrent simulated
clients × 200 `MATCH (n:Label) RETURN count(n)` queries each against a
500-node seeded graph, two runs per side on this development machine
while a concurrent unrelated Docker build shared the CPU — absolute
numbers are machine-dependent, the *ratio* is the signal):

| | qps @ 8 clients (runs) | avg |
|---|---|---|
| Before (exclusive `engine.write().await` per read) | 681.4, 652.6 | ~667 |
| After (lock-free `Executor` clone + `spawn_blocking`) | 1812.7, 1640.4 | ~1727 |
| **Speedup** | | **~2.6x** |

Below the ≥3x stretch target quoted in the proposal (the narrow bare-`RETURN`
fix that motivated the estimate had no catalog/session-check overhead at
all); still a substantial, real reduction in lock contention on the dominant
MATCH path. The gap versus the estimate is consistent with the residual
per-request cost this change did **not** remove: a `parking_lot`-guarded
session-map lookup + a full `Session` clone (`SessionManager::get_session`)
on every read to check "is there an open explicit transaction", and Tokio's
own `spawn_blocking` scheduling overhead. Both are candidates for further
work if additional gains are needed; see the corresponding rulebook task's
`tasks.md` for the exact commands used to reproduce these numbers.
