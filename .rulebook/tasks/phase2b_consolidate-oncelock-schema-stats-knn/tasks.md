## 1. Implementation
- [ ] 1.1 Audit the `CATALOG`, `LABEL_INDEX`, `KNN_INDEX`, `ENGINE`, `EXECUTOR` call sites in `api/schema.rs`, `api/stats.rs`, `api/knn.rs`; verify each one has a matching `NexusServer` field (`engine.catalog`, `engine.indexes.label_index`, `engine.indexes.knn_index`, `engine`, `executor`)
- [ ] 1.2 Migrate schema handlers: `create_label`, `list_labels`, `create_rel_type`, `list_rel_types`, and friends to `State<Arc<NexusServer>>`
- [ ] 1.3 Migrate stats handlers: `get_stats` and every other endpoint that reads the catalog / label index / knn index / engine
- [ ] 1.4 Migrate KNN handlers: `/knn_search`, `/knn_traverse` and the parameterised variants
- [ ] 1.5 Delete the six OnceLock statics, their `init_*` / `get_*` helpers, and the corresponding calls from `nexus-server/src/main.rs`
- [ ] 1.6 Update every `#[tokio::test]` in the three modules to build a real `Arc<NexusServer>` instead of calling `init_*`
- [ ] 1.7 `cargo +nightly build -p nexus-server` clean; `cargo +nightly clippy -p nexus-server --all-targets -- -D warnings` clean

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation: extend the `docs/ARCHITECTURE.md` note from phase2a to mention schema/stats/knn route over `NexusServer` as well
- [ ] 2.2 Write tests covering the new behavior: a parallel-isolation `#[tokio::test]` for each migrated module (build two `NexusServer`s in one binary, prove they do not share catalog/index state)
- [ ] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-server --lib api::schema` + `... api::stats` + `... api::knn` + the integration targets that exercise these routes
