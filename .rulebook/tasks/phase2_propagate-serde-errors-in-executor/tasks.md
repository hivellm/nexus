## 1. Implementation
- [ ] 1.1 Grep `executor/mod.rs` for `unwrap_or_default|unwrap_or_else\(` near `serde_json::` and catalogue every site
- [ ] 1.2 For each site, decide policy: propagate error OR fallback + warn + metric
- [ ] 1.3 Replace per-site according to policy; use `Error::CypherExecution` or a new variant if clearer
- [ ] 1.4 Remove `let _ = cache.write().warm_cache_lazy()` at line 158 — at minimum wrap with `tracing::warn!`
- [ ] 1.5 Add counter `executor_serde_fallback_total` to the Prometheus exporter

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/specs/cypher-subset.md` listing new error paths
- [ ] 2.2 Write targeted executor tests that feed values serde_json can't round-trip and assert the new behaviour (error or warn)
- [ ] 2.3 Run `cargo test --package nexus-core executor::` and confirm pass
