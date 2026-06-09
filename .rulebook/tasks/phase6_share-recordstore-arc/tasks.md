## 1. Investigation
- [ ] 1.1 Map every reader/writer of `engine.storage` and `executor.shared.store`; confirm `RecordStore::clone` (storage/mod.rs:1708) is the per-write mmap cost and that the two are separate copies today
- [ ] 1.2 Decide the shared-handle shape (`Arc<RwLock<RecordStore>>` vs interior-mutable RecordStore) that preserves the single-writer + mmap durability model

## 2. Implementation
- [ ] 2.1 Make `engine.storage` a shared handle and thread the same handle into `ExecutorShared`/`Executor::new`
- [ ] 2.2 Reduce `refresh_executor` to Arc handle swaps (label/knn index) and remove redundant refresh_executor calls now that the store is shared

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation (CHANGELOG / GH #16)
- [ ] 3.2 Write tests: write-then-read visibility across engine + executor with the shared store; concurrency + durability paths unchanged; a guard that a write no longer triggers a RecordStore reopen
- [ ] 3.3 Run tests and confirm they pass (full workspace)
