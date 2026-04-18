## 1. Implementation
- [ ] 1.1 Audit every `parking_lot::RwLock` declaration reachable from an `async fn` (grep `parking_lot::RwLock` in `nexus-server/src/api/`, `nexus-core/src/database.rs`)
- [ ] 1.2 Replace `Arc<parking_lot::RwLock<DatabaseManager>>` with `Arc<tokio::sync::RwLock<DatabaseManager>>`
- [ ] 1.3 Update all `.read()`/`.write()` call sites inside async functions to `.read().await` / `.write().await`
- [ ] 1.4 For any remaining sync callers, wrap the heavy path in `tokio::task::spawn_blocking`
- [ ] 1.5 Enable `clippy::await_holding_lock` = "deny" in workspace lints so regressions fail CI

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/performance/MEMORY_TUNING.md` or create `docs/performance/CONCURRENCY.md` documenting the lock model
- [ ] 2.2 Write a regression test that fires ≥32 concurrent `/cypher` requests and asserts none exceed a latency cap (proves no thread starvation)
- [ ] 2.3 Run `cargo test --workspace` and confirm all tests pass
