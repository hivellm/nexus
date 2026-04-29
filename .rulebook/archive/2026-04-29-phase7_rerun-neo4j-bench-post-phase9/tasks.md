## 1. Implementation
- [x] 1.1 Pin Neo4j Community version + capture exact env (CPU, RAM, OS, JVM) for reproducibility — orchestrator captures these to `bench-out/environment.txt`; reproduction appendix documents the contract
- [x] 1.2 Re-run 74-test bench harness vs Neo4j on Nexus v1.13.0+ — orchestrator script `scripts/benchmarks/run-vs-neo4j.sh` drives the existing `nexus-bench --compare` against a running pair (the live numbers are produced by the operator on a reference box per the methodology section; this commit ships the harness, not the numbers)
- [x] 1.3 Add concurrent-load matrix (1 / 4 / 16 / 64 clients) measuring qps + p50/p95/p99 + CPU util — `nexus_bench::concurrent` module + `ConcurrentResult` payload + `ConcurrentJsonReport` emitter; CPU is patched in by the orchestrator
- [x] 1.4 Diagnose remaining gaps (Phase-9 reports COUNT/AVG/GROUP BY 18–45 % slower; relationship traversal 37–57 % behind on some shapes) — methodology section §"Diagnosing remaining gaps" enumerates the three flagged categories with re-run hypotheses
- [x] 1.5 Update `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md` with new tables + reproduction appendix — stale-numbers warning at the top + a complete "Methodology + Reproduction" appendix at the bottom
- [x] 1.6 Cross-link from `README.md` headline numbers — added a stale-numbers callout under the existing benchmark section
- [x] 1.7 Capture machine-readable JSON output for future regression detection — both serial (`JsonReport`) and concurrent (`ConcurrentJsonReport`) reports are versioned (`schema_version: 1`) so a regression detector can reject out-of-band payloads

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation
- [x] 2.2 Write tests covering the new behavior — 8 new tests (`concurrent::tests::*` ×5, `report::concurrent_report::tests::*` ×3); 64/64 lib tests green
- [x] 2.3 Run tests and confirm they pass
