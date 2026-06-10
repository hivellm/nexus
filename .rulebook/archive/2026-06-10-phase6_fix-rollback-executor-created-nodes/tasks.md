## 1. Implementation
- [x] 1.1 Extend the ROLLBACK arm to undo nodes/relationships in the session watermark range (union with the tracked created lists), deleting relationships before nodes and evicting from label/property indexes — done in engine/transactions.rs (union gated on an active transaction so a stray ROLLBACK can never sweep a stale watermark). SECOND FINDING fixed alongside: BEGIN/COMMIT/ROLLBACK/SAVEPOINT sent over POST /cypher matched no server handler branch and fell through to the bare executor clone, which silently no-opped them — the handler now routes transaction-command ASTs through engine.execute_cypher (nexus-server handler.rs).
- [x] 1.2 Verify end-to-end in Docker: BEGIN → CREATE → ROLLBACK → MATCH count returns 0 — verified on the rebuilt image: rolled-back CREATE invisible (count 0), pre-existing data intact, ROLLBACK without BEGIN now errors, rolled-back data still gone after a container restart

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [Unreleased] Fixed entries (engine rollback sweep + server transaction routing)
- [x] 2.2 Write tests covering the new behavior — `rollback_undoes_executor_created_nodes` (engine: rolled-back CREATE invisible, pre-existing data intact, engine usable after); server-side routing exercised end-to-end in the Docker validation battery (11/11 + durability checks)
- [x] 2.3 Run tests and confirm they pass — nexus-core lib 2384/2384; nexus-server suites green; clippy 0 warnings both crates; Docker battery 11/11 + 4/4 durability
