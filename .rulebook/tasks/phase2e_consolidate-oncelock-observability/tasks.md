## 1. Implementation
- [ ] 1.1 Extend `NexusServer` with `start_time: Instant` and `metrics: Arc<PrometheusMetrics>` (or equivalent Arc-wrapped struct); capture the start-time at construction
- [ ] 1.2 Migrate `health.rs::uptime()` / `health.rs::health_check()` to read `server.start_time` via `State<Arc<NexusServer>>`
- [ ] 1.3 Migrate `prometheus.rs::prometheus_metrics()` to read `server.metrics`; adjust `record_query` / `record_cache_hit` / `increment_connections` / `decrement_connections` call sites across the crate to go through the same handle
- [ ] 1.4 Delete the `START_TIME` and `METRICS` OnceLock statics plus `init()` / `get_metrics()` helpers; drop the `api::prometheus::init()` call from `main.rs`
- [ ] 1.5 Add a compile-time-enforced guard: `nexus-server/tests/no_oncelock_globals.rs` greps the `nexus-server/src/api` tree and fails if any `static .*OnceLock` resurfaces
- [ ] 1.6 `cargo +nightly build -p nexus-server` + `cargo +nightly clippy -p nexus-server --all-targets -- -D warnings` clean

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation: close out the `docs/ARCHITECTURE.md` section saying every `nexus-server` subsystem lives on `NexusServer`, cross-reference the guard test
- [ ] 2.2 Write tests covering the new behavior: the guard test from 1.5 plus one `#[tokio::test]` proving two `NexusServer`s keep independent Prometheus counters (increment on A, assert zero on B)
- [ ] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-server` full lib + integration + the new guard test
