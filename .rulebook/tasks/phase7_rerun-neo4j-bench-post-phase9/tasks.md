## 1. Implementation
- [ ] 1.1 Pin Neo4j Community version + capture exact env (CPU, RAM, OS, JVM) for reproducibility
- [ ] 1.2 Re-run 74-test bench harness vs Neo4j on Nexus v1.13.0+
- [ ] 1.3 Add concurrent-load matrix (1 / 4 / 16 / 64 clients) measuring qps + p50/p95/p99 + CPU util
- [ ] 1.4 Diagnose remaining gaps (Phase-9 reports COUNT/AVG/GROUP BY 18–45 % slower; relationship traversal 37–57 % behind on some shapes)
- [ ] 1.5 Update `docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md` with new tables + reproduction appendix
- [ ] 1.6 Cross-link from `README.md` headline numbers
- [ ] 1.7 Capture machine-readable JSON output for future regression detection

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
