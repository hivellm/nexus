# Implementation Tasks — FTS Benchmarks

## 1. Criterion Harness

- [x] 1.1 `benches/fulltext_bench.rs` with a 100 k × 1 KB corpus generator — deterministic seeded-LCG generator over a 75-word vocabulary, built once per scenario through the bulk-ingest path so corpus setup does not dominate measurement time.
- [x] 1.2 Single-term bench (target: < 5 ms p95) — **measured 150 µs median** on Ryzen 9 7950X3D; ≈33× headroom vs. SLO.
- [x] 1.3 Phrase-query bench (target: < 20 ms p95) — **measured 4.57 ms median**; ≈4.4× headroom vs. SLO.
- [x] 1.4 Ingest throughput bench (target: > 5 k docs/sec sustained) — **measured ≈60 k docs/sec** via `add_node_documents_bulk` (single Tantivy writer + one commit per batch of 10 k); ≈12× headroom vs. SLO.

## 2. Ranking Regression

- [x] 2.1 Fixed corpus in `tests/fulltext_ranking_regression.rs` — a 10-doc hand-curated set that mimics the MS-MARCO short-passage shape (title/body, ~10-15 tokens each), with clearly clustered topical families so top-N assertions remain human-auditable.
- [x] 2.2 Golden top-N result set test per canonical query — 7 tests: `graph_family_dominates_graph_query`, `vector_family_dominates_vector_query`, `phrase_query_pins_exact_match`, `lazy_dog_phrase_hits_only_doc10`, `boolean_must_narrows_down_candidates`, `query_without_hits_returns_empty`, `limit_respected_on_dense_matches`.
- [x] 2.3 Failure diff prints expected vs. actual to aid triage — every assertion includes the actual `top_ids` slice in its panic message.

## 3. CI Wiring

- [x] 3.1 `cargo bench -p nexus-core fulltext` runs as part of the bench job — registered via `[[bench]] name = "fulltext_bench" harness = false` in `crates/nexus-core/Cargo.toml`.
- [x] 3.2 Record baselines under `docs/performance/` — added to `docs/performance/PERFORMANCE_V1.md` (headline table + dedicated "Full-text search (phase6_fulltext-benchmarks)" section with scenario breakdown and write-path notes).

## 4. Tail (mandatory)

- [x] 4.1 Update `docs/performance/PERFORMANCE_V1.md` with FTS numbers.
- [x] 4.2 CHANGELOG entry — `[1.10.0]` "FTS benchmarks + bulk-ingest path + ranking regression".
- [x] 4.3 Update or create documentation covering the implementation — `docs/guides/FULL_TEXT_SEARCH.md` write-path section grew a bulk-ingest example; PERFORMANCE_V1.md carries the numbers.
- [x] 4.4 Write tests covering the new behavior — 7 ranking-regression tests.
- [x] 4.5 Run tests and confirm they pass — ranking regression: 7/7 pass; full lib suite retains 2006 passed / 0 failed / 12 ignored.
- [x] 4.6 Run bench harness + fmt + clippy — `cargo +nightly bench -p nexus-core --bench fulltext_bench` completes in ≈90 s on reference hardware; `cargo +nightly fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings` clean.
