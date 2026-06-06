## 1. Investigation
- [ ] 1.1 Map the MERGE planning path (planner/) and the MERGE executor operator
- [ ] 1.2 Pinpoint the node-MERGE existence lookup — confirm it scans all nodes vs index seek
- [ ] 1.3 Pinpoint the edge-MERGE existence lookup — confirm it scans all relationships vs source adjacency
- [ ] 1.4 Confirm the O(n²) with a reproduction (batch of M merges over a graph of N; measure scaling)

## 2. Implementation — node MERGE
- [ ] 2.1 MERGE planner emits an index seek (label + property / composite B-tree) when a covering index exists
- [ ] 2.2 Correct fallback + retain the unindexed-property warning when no index exists

## 3. Implementation — edge MERGE
- [ ] 3.1 Edge-MERGE existence check uses the source node's typed adjacency, not a global relationship scan
- [ ] 3.2 Verify MERGE semantics preserved (create-if-absent, match-if-present, ON CREATE/ON MATCH)

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation
- [ ] 4.2 Write tests covering the new behavior (correctness + a scaling/benchmark guard showing ~linear, not quadratic)
- [ ] 4.3 Run tests and confirm they pass

## 5. Release unblock
- [ ] 5.1 Fix the flaky `unindexed_property_notification_e2e_test` (notifications leak across consecutive queries / test isolation) so the pre-push gate is green
