# Proposal: phase5_lock-free-read-path

## Why

Bottleneck #1 in [docs/nexus/03-performance.md](../../../docs/nexus/03-performance.md):
every query containing MATCH takes `server.engine.write().await` — the full
exclusive engine lock — serializing all reads against each other and against
writes, server-wide, pinning CPU utilization to effectively one core (~12%
observed). The lock-free `Arc<Executor>` clone + `spawn_blocking` path already
exists but only bare no-pattern queries reach it. The same fix class applied
to a narrow path already produced 3.7x (162→603 qps); the dominant MATCH path
should gain at least as much under concurrency. This is the single
highest-leverage performance change available.

## What Changes

Route autocommit read-only queries (MATCH/OPTIONAL MATCH/RETURN/WITH/UNWIND
with no write clause, no DDL, not inside an explicit transaction) through the
lock-free executor path. Writes and in-transaction reads keep the engine lock
(single-writer ordering + read-your-own-writes preserved). Routing decision
uses the shared AST predicate from `api/cypher/routing.rs` (built in
phase1_http-merge-rel-and-set-rel-parity) — not string matching.

## Impact

- Affected specs: specs/read-path/spec.md (this task)
- Affected code: `crates/nexus-server/src/api/cypher/execute/handler.rs`,
  `crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs`; possibly
  executor snapshot-refresh wiring in `crates/nexus-core`
- Breaking change: NO (semantics preserved: MVCC snapshot reads; explicit
  transactions unchanged)
- User benefit: concurrent read throughput scales with cores instead of
  serializing on one lock; p95 latency under load drops.

## Success criteria (gate)

Concurrency benchmark (N parallel clients, MATCH workload) before/after:
target ≥3x throughput at 8+ clients and near-linear scaling to core count.
Snapshot-visibility tests prove no read-your-own-write regression within a
session/transaction.
