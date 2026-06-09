## 1. Investigation
- [ ] 1.1 Confirm explicit `COMMIT` calls `rebuild_indexes_from_storage` (mod.rs:3831) and that `apply_pending_index_updates` (mod.rs:3817) already applies the incremental index updates for the committed session
- [ ] 1.2 Confirm rollback path does not depend on the post-commit rebuild for index consistency

## 2. Implementation
- [ ] 2.1 Remove the per-COMMIT `rebuild_indexes_from_storage()` call; keep `refresh_executor` (or make it incremental per #16)
- [ ] 2.2 Add a dev/test assertion that the indexes after incremental commit match a full rebuild (no net diff)

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation (CHANGELOG Fixed / GH #15)
- [ ] 3.2 Write tests: explicit BEGIN/COMMIT keeps label + relationship + property indexes correct without a full rebuild; commit cost does not scale with total graph size
- [ ] 3.3 Run tests and confirm they pass
