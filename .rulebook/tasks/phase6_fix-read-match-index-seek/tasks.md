## 1. Investigation
- [ ] 1.1 Reproduce: single-node `MATCH (n:Turn {id:"nope"})` with a covering index = full scan (hundreds of ms)
- [ ] 1.2 Reproduce: comma-join `MATCH (a:L1 {..}), (b:L2 {..})` plans as a cartesian product of two label scans (O(N^2)/timeout)
- [ ] 1.3 Map the read path: `execute_node_by_label` + filter, the `try_index_based_filter` stub, and multi-pattern MATCH planning in queries.rs

## 2. Implementation — single-node index seek
- [ ] 2.1 Read-side `MATCH (n:Label {prop: val})` uses `property_index.find_exact` (intersect per-property bitmaps) when a covering index exists; falls back to label scan otherwise
- [ ] 2.2 Reuse/share the index-seek logic with the MERGE path where practical

## 3. Implementation — comma-join / multi-pattern MATCH
- [ ] 3.1 Push each node pattern's property predicate into its own leg so endpoints are independent index seeks, not a cartesian product
- [ ] 3.2 Verify the edge-upsert pattern `MATCH (a:L1 {id}), (b:L2 {id}) MERGE (a)-[r:T]->(b)` resolves endpoints via index seeks

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation (CHANGELOG / GH #8)
- [ ] 4.2 Write tests: correctness + a scaling guard (single-node seek O(log N); two-node comma-join not cartesian)
- [ ] 4.3 Run tests and confirm they pass
