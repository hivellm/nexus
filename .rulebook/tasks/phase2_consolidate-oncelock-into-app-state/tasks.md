## 1. Implementation
- [ ] 1.1 Enumerate every `OnceLock<Arc<_>>` in `nexus-server/src/api/*` and list which module owns each (catalogue file)
- [ ] 1.2 Introduce `nexus-server/src/state.rs` exposing `pub struct AppState` + `impl AppState { pub fn new(...) -> Result<Self> }`
- [ ] 1.3 Refactor `main.rs::async_main` to build `AppState` once and attach via `.with_state(Arc::new(state))`
- [ ] 1.4 Migrate handlers one sub-API at a time (cypher → data → performance → graph_correlation → …) to take `State<Arc<AppState>>`
- [ ] 1.5 Delete the per-module `init_*` / `get_*` globals and `OnceLock` statics as each migration lands

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/ARCHITECTURE.md` describing the new `AppState` ownership model
- [ ] 2.2 Add an integration test that builds two independent `AppState` instances in the same test binary and proves they don't share state
- [ ] 2.3 Run `cargo test --workspace` and confirm all tests pass
