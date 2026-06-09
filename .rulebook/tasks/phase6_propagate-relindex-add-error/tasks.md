## 1. Investigation
- [ ] 1.1 Confirm the swallowed error at crud.rs:992-998 and trace the downstream effect (missing exact-edge entry -> find_relationship_between O(degree) fallback -> possible duplicate MERGE edge)
- [ ] 1.2 Decide propagate-error vs dirty-bit-forced-rebuild (which preserves write success while guaranteeing index/storage consistency)

## 2. Implementation
- [ ] 2.1 Stop silently swallowing the `relationship_index().add_relationship` error on the create_relationship path (propagate or set a rebuild dirty-bit consumed before the next query)
- [ ] 2.2 Keep the phase-8 manager updates logged; apply the same guarantee if low-cost

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation (CHANGELOG / GH #18)
- [ ] 3.2 Write tests: a forced relationship-index add failure does not leave MERGE able to create a duplicate edge / silently fall back (existence stays correct)
- [ ] 3.3 Run tests and confirm they pass
