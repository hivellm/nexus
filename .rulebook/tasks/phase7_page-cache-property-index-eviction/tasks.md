## 1. Implementation
- [ ] 1.1 Read `crates/nexus-core/src/cache/mod.rs` to map the existing placeholder branch + lookup path
- [ ] 1.2 Decide policy (LRU recommended; TTL secondary) and document choice via `rulebook_decision_create`
- [ ] 1.3 Implement LRU slice for property-index entries, sized via env `NEXUS_PROPERTY_INDEX_CACHE_MB` (default 64)
- [ ] 1.4 Add hit/miss/eviction counters; wire into `/stats` JSON shape
- [ ] 1.5 Replace the `// Check if index is actually cached` placeholder by implementing the branch correctly
- [ ] 1.6 Update `docs/specs/page-cache.md` with the property-index-slice section
- [ ] 1.7 Add integration test: N property indexes filled past cap, assert RSS stable within tolerance
- [ ] 1.8 Run `cargo clippy --workspace -- -D warnings` and `cargo +nightly fmt --all`

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
