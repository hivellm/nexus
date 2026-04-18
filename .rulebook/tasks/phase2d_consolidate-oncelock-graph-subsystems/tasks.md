## 1. Implementation
- [ ] 1.1 Audit handlers in `api/graph_correlation.rs`, `api/graph_correlation_umicp.rs`, `api/comparison.rs` to list every function that reads `GRAPH_MANAGER`, `UMICP_HANDLER`, `GRAPH_A`, `GRAPH_B`
- [ ] 1.2 Extend `NexusServer` with the four fields plus builder args; update `NexusServer::new` and every caller (main.rs + existing test fixtures)
- [ ] 1.3 Move the construction logic currently in `api::comparison::init_graphs` and `api::graph_correlation::init_manager` into helpers the server constructor can call; the `GraphUmicpHandler` Arc is straight-forward to default-construct
- [ ] 1.4 Migrate handlers to `State<Arc<NexusServer>>`; replace reads of `GRAPH_MANAGER.get()`, `GRAPH_A.get()`, etc. with `server.graph_correlation_manager` and friends
- [ ] 1.5 Delete the four OnceLock statics and the `init_*` / `get_*` helpers
- [ ] 1.6 Remove the init calls from `main.rs::async_main`
- [ ] 1.7 `cargo +nightly build -p nexus-server` + `cargo +nightly clippy -p nexus-server --all-targets -- -D warnings` clean

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation: note the graph-correlation / UMICP / comparison state entries in `docs/ARCHITECTURE.md`
- [ ] 2.2 Write tests covering the new behavior: a `#[tokio::test]` that builds two independent `NexusServer`s and exercises `GraphA` on each, proving they do not share the comparison state
- [ ] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-server --lib api::graph_correlation` + `... api::graph_correlation_umicp` + `... api::comparison` + the relevant integration targets
