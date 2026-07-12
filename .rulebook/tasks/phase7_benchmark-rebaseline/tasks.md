## 1. vs-Neo4j re-baseline
- [ ] 1.1 Pin versions + hardware profile; re-run the full vs-Neo4j suite (`scripts/benchmarks/run-vs-neo4j.sh`)
- [ ] 1.2 Resolve the CREATE-relationship contradiction (87.6% slower vs 42.7x faster) with a dedicated repeated-run scenario; record the verdict
- [ ] 1.3 Mark `BENCHMARK_NEXUS_VS_NEO4J.md` (Dec-2025) as superseded; publish consolidated `docs/performance/BENCHMARK_2026.md`

## 2. KNN recall publication
- [ ] 2.1 Run the SIFT1M and GloVe-200d recall@k vs latency measurements per `KNN_RECALL.md` methodology
- [ ] 2.2 Publish curves + numbers into `KNN_RECALL.md`; update or correct the "<2ms p95 / 10K qps" claims wherever cited (CLAUDE.md, README)

## 3. Per-transport compatibility suite
- [ ] 3.1 Extend the compat suite to run the same battery over HTTP, RPC, RESP3, and GraphQL-mutation subset, diffing results across transports
- [ ] 3.2 Wire the per-transport run into CI as the permanent write-path regression net

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation (canonical benchmark doc + corrected claims)
- [ ] 4.2 Write tests covering the new behavior (per-transport suite is the test; assert cross-transport equality)
- [ ] 4.3 Run tests and confirm they pass
