## 1. Implementation
- [ ] 1.1 Filter null-valued keys out of the inline-property map before it reaches `store_properties` / `store_properties_if_any` on the CREATE path, and derive `inline_prop_count` from the filtered map instead of its own private `is_null()` filter (both sites: create.rs:130 and create.rs:319)
- [ ] 1.2 Apply the same rule to the update paths: `SET n.p = null` removes an existing key (`-properties 1`) and is a no-op on an absent one; `SET n = {map}` / `SET n += {map}` / MERGE `ON CREATE` + `ON MATCH` never store a null-valued key
- [ ] 1.3 Regression tests that assert observable graph state, not just the counter — `keys(n)`, `properties(n)`, `n.p IS NULL` — alongside the `+properties` / `-properties` values, for every path touched in 1.1 and 1.2

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
