# Proposal: phase6_fulltext-benchmarks

## Why

v1.8 ships FTS without performance guardrails. The original spec
set concrete SLOs: single-term query < 5 ms p95, phrase < 20 ms
p95, and sustained ingest > 5 k docs/sec on a 100 k × 1 KB corpus.
Without a Criterion harness + a ranking regression corpus, we
cannot detect regressions introduced by the WAL-integration and
analyzer-catalogue follow-ups, nor prove the SLOs to downstream
users.

## What Changes

1. Add a Criterion bench harness under `benches/fulltext.rs` with
   three scenarios:
   - Single-term query over 100 k × 1 KB corpus (target: < 5 ms p95).
   - Phrase query over the same corpus (target: < 20 ms p95).
   - Ingest throughput (target: > 5 k docs/sec sustained).
2. Generate or import a fixed MS MARCO sample used by both the
   bench corpus and a ranking regression test that asserts the
   top-10 result set is stable across releases.
3. Wire `cargo bench -p nexus-core fulltext` into the repo-level
   bench runner.
4. Document the bench targets and the recorded baselines in
   `docs/performance/PERFORMANCE_V1.md`.

## Impact

- Affected specs: `docs/performance/PERFORMANCE_V1.md`.
- Affected code: `crates/nexus-core/benches/fulltext.rs` (new), `crates/nexus-core/tests/fulltext_ranking.rs` (new).
- Breaking change: NO.
- User benefit: SLO visibility for FTS; regression guard on the ranking output; stable numbers downstream teams can quote.
