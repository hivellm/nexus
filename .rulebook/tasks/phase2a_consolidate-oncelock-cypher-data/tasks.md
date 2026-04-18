## 1. Implementation
- [ ] 1.1 Audit every call site under `nexus-server/src/api/cypher/*.rs` + `nexus-server/src/api/data.rs` that reads `EXECUTOR`, `ENGINE`, `CATALOG`, `DATABASE_MANAGER`, and note which one corresponds to which `NexusServer` field
- [ ] 1.2 Update handler signatures in `api/cypher/execute.rs` and `api/cypher/commands.rs` to take `State(server): State<Arc<NexusServer>>` (or an `Extension`-based equivalent where the extractor isn't in the chain); replace global reads with `server.engine`, `server.executor`, `server.database_manager`
- [ ] 1.3 Do the same across every handler in `api/data.rs` (get/create/update/delete/search/ingest variants)
- [ ] 1.4 Move the `enable_query_cache_with_config` dance currently inside `api::cypher::init_executor` into a public `Executor` constructor (or a `NexusServer` setup path) so `main.rs` still wires the cache without the global
- [ ] 1.5 Delete the OnceLock statics and `init_*` / `get_*` helpers in both modules; remove the `api::cypher::init_executor` / `init_engine` / `init_database_manager` and `api::data::init_engine` calls from `main.rs`
- [ ] 1.6 Update every in-module test (`api::cypher::tests`, `api::data::tests`) that previously called `init_engine(...)` to build a throwaway `Arc<NexusServer>` directly; ensure the tests stay parallel-safe
- [ ] 1.7 `cargo +nightly build -p nexus-server` clean; `cargo +nightly clippy -p nexus-server --all-targets -- -D warnings` clean

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation: add a short section to `docs/ARCHITECTURE.md` describing that `NexusServer` is the canonical state handle and that cypher/data handlers read from it via Axum's `State` extractor
- [ ] 2.2 Write tests covering the new behavior: at least one `#[tokio::test]` per migrated module that constructs two independent `Arc<NexusServer>` values in the same test binary and proves they do not share state (e.g. create a node in server A, assert it is not visible in server B)
- [ ] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-server --lib api::cypher` + `cargo +nightly test -p nexus-server --lib api::data` + the integration targets that exercise those routes
