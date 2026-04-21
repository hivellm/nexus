# Implementation Tasks — FTS Benchmarks

## 1. Criterion Harness

- [ ] 1.1 `benches/fulltext.rs` with a 100 k × 1 KB corpus generator
- [ ] 1.2 Single-term bench (target: < 5 ms p95)
- [ ] 1.3 Phrase-query bench (target: < 20 ms p95)
- [ ] 1.4 Ingest throughput bench (target: > 5 k docs/sec sustained)

## 2. Ranking Regression

- [ ] 2.1 Import / vendor a fixed MS MARCO sample into `tests/fixtures/`
- [ ] 2.2 Golden top-10 result set test per canonical query
- [ ] 2.3 Failure diff prints expected vs. actual to aid triage

## 3. CI Wiring

- [ ] 3.1 `cargo bench -p nexus-core fulltext` runs as part of the bench job
- [ ] 3.2 Record baselines under `docs/performance/`

## 4. Tail (mandatory)

- [ ] 4.1 Update `docs/performance/PERFORMANCE_V1.md` with FTS numbers
- [ ] 4.2 CHANGELOG entry
- [ ] 4.3 Run bench harness + fmt + clippy
