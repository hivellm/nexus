## 1. Dataset expansion

- [x] 1.1 `SmallDataset` — 50 nodes + 53 edges as a single CREATE literal (~2 KiB). Hub-plus-chain topology over a single `:P` label and `KNOWS` type; supports deterministic 1-hop / 2-hop / `*1..3` traversal counts
- [ ] 1.2 `VectorSmallDataset` — 50 nodes with 16-dim `score_vec` property **[blocked — HNSW KNN via Cypher not yet exposed]**
- [x] 1.3 Catalogue tests assert every new dataset's literal is a single CREATE statement (same guard as `TinyDataset`) — `small_load_is_single_statement` + `small_load_has_fifty_three_knows_edges` + `small_load_binds_every_node_variable` + `small_load_fits_in_request_body_budget` + `small_load_is_uniform_label` in `crates/nexus-bench/src/dataset.rs`

## 2. Scenario catalogue — split into submodules

- [ ] 2.1 Split `scenario_catalog.rs` into `scenarios/{scalar,aggregation,traversal,write,index,constraint,subquery,procedure,temporal_spatial,hybrid}.rs`
- [ ] 2.2 Keep a single `seed_scenarios()` aggregator that returns the concatenated list

## 3. Traversals (§10)

- [x] 3.1 1-hop neighbour lookup on `SmallDataset` — `traversal.small_one_hop_hub`
- [x] 3.2 2-hop friend-of-friend — `traversal.small_two_hop_from_hub`
- [x] 3.3 Variable-length path `*1..3` — `traversal.small_var_length_1_to_3`
- [ ] 3.4 Quantified path pattern `{1,5}` (once QPP ships)
- [ ] 3.5 `shortestPath` — pending a separate verification that Nexus's shortest-path syntax matches Neo4j
- [ ] 3.6 MATCH with multiple patterns + cartesian join — pending a non-ambiguous query on the shared fixture

## 4. Writes (§11)

- [ ] 4.1 Single-node CREATE
- [ ] 4.2 Batched CREATE via UNWIND (100-row literal)
- [ ] 4.3 MERGE with + without existing match
- [ ] 4.4 SET property
- [ ] 4.5 DELETE / DETACH DELETE

## 5. Indexes (§12)

- [ ] 5.1 Bitmap label scan vs full scan
- [ ] 5.2 B-tree equality + range seek
- [ ] 5.3 Composite B-tree prefix
- [ ] 5.4 HNSW KNN k=1 / k=10 (once vector dataset ships)
- [ ] 5.5 R-tree `withinDistance` (once geospatial predicates ship)
- [ ] 5.6 Full-text single-term (once FTS ships)

## 6. Constraints (§13)

- [ ] 6.1 UNIQUE insert overhead
- [ ] 6.2 NOT NULL insert + SET overhead
- [ ] 6.3 NODE KEY composite check

## 7. Subqueries (§14)

- [ ] 7.1 `EXISTS { }` predicate
- [ ] 7.2 `COUNT { }` subquery
- [ ] 7.3 `COLLECT { }` subquery
- [ ] 7.4 Nested `CALL { }` 3-deep
- [ ] 7.5 `CALL { } IN TRANSACTIONS` throughput (once the clause ships)

## 8. Procedures (§15)

- [ ] 8.1 `db.labels` / `db.indexes` / `db.constraints` latency
- [ ] 8.2 `dbms.procedures` / `dbms.components`
- [ ] 8.3 `apoc.coll.*` representative set
- [ ] 8.4 `apoc.map.*` merge / groupBy
- [ ] 8.5 `apoc.path.expand` vs native variable-length
- [ ] 8.6 `gds.pageRank`

## 9. Temporal & Spatial (§16)

- [ ] 9.1 `date.format` / `duration.between` / `date.truncate`
- [ ] 9.2 `point.distance` WGS-84 + Cartesian
- [ ] 9.3 Spatial `withinDistance` with + without R-tree

## 10. Hybrid / RAG (§17)

- [ ] 10.1 Vector KNN + graph traversal
- [ ] 10.2 Full-text + vector re-ranking
- [ ] 10.3 Graph + spatial + temporal (geofencing over time)

## 11. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 11.1 Update or create documentation covering the implementation — `crates/nexus-bench/README.md` scenario table
- [ ] 11.2 Write tests covering the new behavior — catalogue invariants + `#[ignore]` integration tests per category
- [ ] 11.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-bench --features live-bench` under a running Nexus server
