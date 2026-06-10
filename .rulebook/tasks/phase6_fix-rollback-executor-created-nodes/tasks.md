## 1. Implementation
- [ ] 1.1 Extend the ROLLBACK arm to undo nodes/relationships in the session watermark range (union with the tracked created lists), deleting relationships before nodes and evicting from label/property indexes
- [ ] 1.2 Verify end-to-end in Docker: BEGIN → CREATE → ROLLBACK → MATCH count returns 0

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
