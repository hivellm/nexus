## 1. Implementation
- [x] 1.1 Grep `executor/mod.rs` for `unwrap_or_default|unwrap_or_else\(` near `serde_json::` and catalogue every site
- [x] 1.2 For each site, decide policy: propagate error OR fallback + warn + metric
- [x] 1.3 Replace per-site according to policy; use `Error::CypherExecution` or a new variant if clearer
- [x] 1.4 Remove `let _ = cache.write().warm_cache_lazy()` at line 158 — at minimum wrap with `tracing::warn!`
- [x] 1.5 Add counter `executor_serde_fallback_total` to the Prometheus exporter

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation: `docs/specs/cypher-subset.md` Error Handling section lists the new GROUP BY / DISTINCT / UNION error paths and the `nexus_executor_serde_fallback_total` counter
- [x] 2.2 Write tests covering the new behavior: `serde_metrics::tests::*`, `executor::tests::aggregate_group_by_propagates_serde_failure`, `executor::tests::serde_metrics_snapshot_is_monotonic`
- [x] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-core -p nexus-server`
