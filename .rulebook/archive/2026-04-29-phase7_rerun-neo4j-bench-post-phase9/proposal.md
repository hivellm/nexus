# Proposal: phase7_rerun-neo4j-bench-post-phase9

## Why

The 74-test cross-bench numbers in `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md` were captured Dec-2025 against Nexus v0.12.0 — *before* the Phase-9 fix that lifted mixed-read throughput from 162 → 603.91 qps (3.7×). The currently-published "Nexus 4.15× avg, 42.74× max vs Neo4j" headline still uses pre-fix data, and the global-executor-lock issue documented in `PERFORMANCE_ANALYSIS.md` (Nexus 202 qps vs Neo4j 507 qps, ~12 % CPU vs ~80 %) was the dominant ceiling at measurement time. Without a re-run we cannot honestly claim concurrent-workload performance, and the marketing numbers are stale.

## What Changes

- Re-run `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` and the 74-test bench harness against Nexus v1.13.0+ (post-Phase-9) and a current Neo4j Community release.
- Add a concurrent-load section: 1, 4, 16, 64 concurrent clients on the same workload set; report qps + p50/p95/p99 + CPU utilization side-by-side.
- Update `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md` with the new numbers and an explicit "Methodology + Reproduction" appendix.
- Publish a v1.2 perf summary doc cross-linked from README.

## Impact

- Affected specs: `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md`, `docs/performance/PERFORMANCE_V1.md` (cross-link).
- Affected code: bench harness only (`scripts/benchmarks/`); no engine changes.
- Breaking change: NO.
- User benefit: trustworthy perf claims; defensible vs a competitor doing their own run.
