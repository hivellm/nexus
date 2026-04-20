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
- [ ] 3.5 `shortestPath` — **[blocked]** the `shortestPath((…)-[*]->(…))` syntax errors at Nexus's parser (column 25). Tracked in `phase6_nexus-bench-correctness-gaps`. Scenario was added and then pulled; add back after the fix ships
- [x] 3.6 MATCH with multiple patterns + cartesian join — `traversal.cartesian_a_b` (commit `6a9983f4`). Content-matches Neo4j; exposed a 287× performance gap on Nexus noted in the gaps task

## 4. Writes (§11)

- [x] 4.1 Single-node CREATE — `write.create_singleton` (commit `6a9983f4`); idempotent literal return so the divergence guard stays useful across iterations
- [ ] 4.2 Batched CREATE via UNWIND (100-row literal) — pending; needs a query shape that returns the same count across iterations
- [x] 4.3 MERGE with + without existing match — `write.merge_singleton` (commit `6a9983f4`); idempotent by design
- [x] 4.4 SET property — `write.set_property` (commit `6a9983f4`); idempotent SET on a known node
- [ ] 4.5 DELETE / DETACH DELETE — `write.create_delete_cycle` added but hits `phase6_nexus-bench-correctness-gaps` §8 (Nexus rejects DELETE on CREATE→WITH-flow bindings). Content-matches Neo4j once §8 ships

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

- [ ] 7.1 `EXISTS { }` predicate — pending; `subquery.exists_high_score` uses the older `MATCH → WITH → RETURN` form and is tracked under `phase6_nexus-bench-correctness-gaps` §5 for the WITH→RETURN regression. Promote to the `EXISTS { }` syntax once the WITH bug is fixed
- [ ] 7.2 `COUNT { }` subquery — pending
- [x] 7.3 `COLLECT { }` subquery — partial; `subquery.collect_names` + `subquery.size_of_collect` exercise the collect-then-project path (even if the latter surfaces the §5 bug)
- [ ] 7.4 Nested `CALL { }` 3-deep — pending
- [ ] 7.5 `CALL { } IN TRANSACTIONS` throughput (once the clause ships)

## 8. Procedures (§15)

- [x] 8.1 `db.labels` / `db.indexes` / `db.constraints` latency — `procedure.db_labels`, `procedure.db_relationship_types`, `procedure.db_property_keys`, `procedure.db_indexes` all landed. Content-wise they're broken today (tracked in `phase6_nexus-bench-correctness-gaps` §3), but the bench rows exist. `db.constraints` pending until the §3 fix makes `db.*` meaningfully yieldable
- [ ] 8.2 `dbms.procedures` / `dbms.components` — pending
- [ ] 8.3 `apoc.coll.*` representative set — **[blocked on APOC]**
- [ ] 8.4 `apoc.map.*` merge / groupBy — **[blocked on APOC]**
- [ ] 8.5 `apoc.path.expand` vs native variable-length — **[blocked on APOC]**
- [ ] 8.6 `gds.pageRank` — **[blocked on GDS]**

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
