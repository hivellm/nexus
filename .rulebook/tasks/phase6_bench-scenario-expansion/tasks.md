## 1. Dataset expansion

- [x] 1.1 `SmallDataset` — 50 nodes + 53 edges as a single CREATE literal (~2 KiB). Hub-plus-chain topology over a single `:P` label and `KNOWS` type; supports deterministic 1-hop / 2-hop / `*1..3` traversal counts
- [ ] 1.2 `VectorSmallDataset` — 50 nodes with 16-dim `score_vec` property **[blocked — HNSW KNN via Cypher not yet exposed]**
- [x] 1.3 Catalogue tests assert every new dataset's literal is a single CREATE statement (same guard as `TinyDataset`)

## 2. Scenario catalogue — split into submodules

- [x] 2.1 Split `scenario_catalog.rs` into `scenarios/{aggregation,filter,label_scan,order,point_read,procedure,scalar,subquery,temporal_spatial,traversal,write}.rs`
- [x] 2.2 Keep a single `seed_scenarios()` aggregator that returns the concatenated list — `crate::scenario_catalog::seed_scenarios` now delegates to `crate::scenarios::all()` so external callers are unaffected

## 3. Traversals (§10)

- [x] 3.1 1-hop neighbour lookup on `SmallDataset` — `traversal.small_one_hop_hub`
- [x] 3.2 2-hop friend-of-friend — `traversal.small_two_hop_from_hub`
- [x] 3.3 Variable-length path `*1..3` — `traversal.small_var_length_1_to_3`
- [ ] 3.4 Quantified path pattern `{1,5}` (once QPP ships)
- [ ] 3.5 `shortestPath` — **[blocked]** Nexus's parser errors on the `shortestPath((…)-[*]->(…))` token (column 25). Tracked in `phase6_nexus-bench-correctness-gaps`. Add back once the parser accepts the Neo4j syntax
- [x] 3.6 MATCH with multiple patterns + cartesian join — `traversal.cartesian_a_b`

## 4. Writes (§11)

- [x] 4.1 Single-node CREATE — `write.create_singleton`; literal return for iteration-safety
- [x] 4.2 Batched CREATE via UNWIND — `write.unwind_create_batch` (`UNWIND range(1,10) AS i CREATE (:BenchBatch {i:i}) RETURN count(*)`). Content-divergent on Nexus (see gaps task — UNWIND+CREATE aggregation bug); scenario row is in place for when the bug closes
- [x] 4.3 MERGE with + without existing match — `write.merge_singleton`
- [x] 4.4 SET property — `write.set_property`
- [x] 4.5 DELETE / DETACH DELETE — `write.create_delete_cycle`. Errors on Nexus today (gaps task §8); scenario row in place

## 5. Indexes (§12)

- [x] 5.1 Bitmap label scan vs full scan — covered by `label_scan.count_a` + `aggregation.count_all` pair; the two scenarios' latency ratio is the label-scan speed-up
- [x] 5.2 B-tree equality + range seek — covered by `point_read.by_id` (equality) + `filter.label_and_id` / `filter.score_range` (range)
- [ ] 5.3 Composite B-tree prefix — **[blocked on composite index feature]**
- [ ] 5.4 HNSW KNN k=1 / k=10 — **[blocked on vector dataset + KNN operator]**
- [ ] 5.5 R-tree `withinDistance` — **[blocked on R-tree feature]**
- [ ] 5.6 Full-text single-term — **[blocked on FTS feature]**

## 6. Constraints (§13)

- [ ] 6.1 UNIQUE insert overhead — **[blocked on constraint runtime enforcement]**
- [ ] 6.2 NOT NULL insert + SET overhead — **[blocked]**
- [ ] 6.3 NODE KEY composite check — **[blocked]**

## 7. Subqueries (§14)

- [ ] 7.1 `EXISTS { }` predicate — **[paired with gaps §5]**; `subquery.exists_high_score` uses the older `MATCH → WITH → RETURN` form because the newer `EXISTS { }` syntax and the WITH→RETURN bug are intertwined. Promote the query shape after the gap closes
- [x] 7.2 `COUNT { }` subquery — `subquery.count_subquery`. Nexus returns null instead of the subquery count (gaps task); scenario row in place
- [x] 7.3 `COLLECT { }` subquery — `subquery.collect_names` + `subquery.size_of_collect`; the latter surfaces the gaps §5 WITH→RETURN bug
- [ ] 7.4 Nested `CALL { }` 3-deep — pending; needs `CALL { }` support verification on Nexus first
- [ ] 7.5 `CALL { } IN TRANSACTIONS` — **[blocked on clause landing]**

## 8. Procedures (§15)

- [x] 8.1 `db.labels` / `db.indexes` / `db.constraints` latency — `procedure.db_labels`, `procedure.db_relationship_types`, `procedure.db_property_keys`, `procedure.db_indexes` scenarios landed. Content-wise they're broken today (gaps task §3). `db.constraints` pending until §3 fix makes `db.*` meaningfully yieldable
- [x] 8.2 `dbms.procedures` / `dbms.components` — `procedure.dbms_components` landed. Nexus does not have the procedure registered (gaps task — new finding in Run 6)
- [ ] 8.3 `apoc.coll.*` — **[blocked on APOC]**
- [ ] 8.4 `apoc.map.*` — **[blocked on APOC]**
- [ ] 8.5 `apoc.path.expand` — **[blocked on APOC]**
- [ ] 8.6 `gds.pageRank` — **[blocked on GDS]**

## 9. Temporal & Spatial (§16)

- [x] 9.1 `date.format` / `duration.between` / `date.truncate` — `scalar.date_literal` + `scalar.duration_between_days` landed. Duration bench-errors on Nexus today (returns 0 rows); filed as part of the temporal/spatial gap in the correctness-gaps task
- [x] 9.2 `point.distance` WGS-84 + Cartesian — `scalar.point_distance_cartesian` + `scalar.point_distance_wgs84`. Both bench-error on Nexus today (return 0 rows); same gap
- [ ] 9.3 Spatial `withinDistance` with + without R-tree — **[blocked on R-tree]**

## 10. Hybrid / RAG (§17)

- [ ] 10.1 Vector KNN + graph traversal — **[blocked on vector dataset]**
- [ ] 10.2 Full-text + vector re-ranking — **[blocked on FTS + vector]**
- [ ] 10.3 Graph + spatial + temporal (geofencing over time) — **[blocked on all three]**

## 11. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 11.1 Update or create documentation covering the implementation — `crates/nexus-bench/README.md` already counts the scenarios at this commit's catalogue size; the scenario table grows implicitly as submodule files land under `src/scenarios/`
- [x] 11.2 Write tests covering the new behavior — catalogue invariants in `scenario_catalog::tests` cover uniqueness / row-count / category coverage / write-prefix intent; `tests/live_rpc.rs::seed_catalog_run_completes` + `tests/live_compare.rs::comparative_seed_catalogue_completes` exercise every scenario against live servers
- [x] 11.3 Run tests and confirm they pass — 6 bench runs landed under `docs/benchmarks/baselines/2026-04-20-run{1..6}.{md,json}` against a live Nexus + docker Neo4j. Every scenario that Nexus doesn't outright reject runs cleanly; every content-divergence or parse rejection is tracked in `phase6_nexus-bench-correctness-gaps`
