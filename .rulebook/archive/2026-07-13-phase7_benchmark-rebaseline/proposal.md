# Proposal: phase7_benchmark-rebaseline

## Why

The benchmark story is currently unpublishable
([docs/nexus/03-performance.md](../../../docs/nexus/03-performance.md)):

- **Contradictory numbers**: CREATE-relationship is reported as 87.6% slower
  than Neo4j (phase9 report) AND 42.7x faster (Dec-2025 report, self-flagged
  stale) — the truth is unknown.
- **KNN claims unverified**: `KNN_RECALL.md` is methodology-only; the
  "<2ms p95 / 10K+ KNN qps" claims in CLAUDE.md have no published
  recall@k/latency curves, unlike Milvus/Qdrant/Weaviate which all publish
  standard curves. The per-label HNSW is Nexus's differentiator — it needs
  numbers to count.
- **Compat suite is single-path**: the 300-test diff suite runs one
  transport, which is exactly why the transport-correctness bugs (B1–B7)
  survived it.

Runs after phase5/phase6 so published numbers include the perf work.

## What Changes

- Re-run the full vs-Neo4j suite (`scripts/benchmarks/run-vs-neo4j.sh`) on
  pinned versions/hardware; resolve the CREATE-rel contradiction; retire the
  stale Dec-2025 report (mark superseded).
- Measure and publish KNN recall@k vs latency on SIFT1M + GloVe-200d per the
  documented methodology in `KNN_RECALL.md`.
- Extend the Neo4j compatibility suite to execute the same battery over
  every transport (HTTP, RPC, RESP3, GraphQL mutations subset) and diff
  results — the permanent regression net for the write-path unification.
- One consolidated `docs/performance/BENCHMARK_2026.md` replacing the
  contradictory generations as the canonical reference.

## Impact

- Affected specs: specs/benchmarks/spec.md (this task)
- Affected code: `crates/nexus-bench` scenarios,
  `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` (per-transport
  mode), docs/performance/
- Breaking change: NO
- User benefit: trustworthy, publishable numbers — required for competitive
  credibility; regression net that keeps transports honest.
