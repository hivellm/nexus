# phase7_opencypher-gap-closure — tasks

> **Revised 2026-07-19** after verifying the original draft. One item was refuted and
> removed (`FOR … REQUIRE` already ships), two were misdiagnosed as parser gaps, and
> the harness prerequisites were promoted ahead of the baseline — a pass rate measured
> with vacuous assertions would be worse than no number at all.

Order matters and is deliberate: make the harness honest, measure a baseline, then fix
gaps, then re-measure so the delta is attributable. Do not reorder §2 before §1.

## 1. Correct the record (cheap, immediate, no code)
- [x] 1.1 Document the shipped-but-undocumented `CREATE CONSTRAINT … FOR (n:L) REQUIRE …` syntax in `docs/specs/cypher-subset.md` — it is fully implemented (`executor/parser/clauses/admin.rs:311-324`, body parser `:413-437`, tests `parser/tests/ddl.rs:186,205,222,239,256`) but absent from the spec, which is what led the original draft to believe it was missing
- [x] 1.2 Fix the stale header in `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` (`:1`, `:21` say "300 Tests"; the file has **325** `Run-Test` calls), and decide the fate of the `.sh` sibling, which has drifted to 196 cases — reconcile it or delete it, but do not leave two suites claiming to be the same thing
- [x] 1.3 Fix the stale `"status": "pending"` in `.rulebook/archive/2026-04-27-phase6_opencypher-subquery-transactions/.metadata.json` (it is archived with all 52 items done)

## 2. Make the harness honest (prerequisite — a baseline measured before this is inflated)
- [ ] 2.1 Implement real side-effect assertions. **Measured against the vendored corpus: 1498 of 1615 scenarios (93%) assert side effects** — 1180 use `And no side effects`, 244 use `And the side effects should be:` with tables like `| +nodes | 1 |`. Skipping them is therefore not an option; it would gut the corpus. The counters the TCK expects are `+nodes`/`-nodes`, `+relationships`/`-relationships`, `+properties`/`-properties`, `+labels`/`-labels`
  **Scope discovery — this is an engine feature, not a harness tweak.** Side-effect counters do not exist anywhere in the codebase: `grep nodes_created` finds nothing in `nexus-core` or `nexus-server`, and `ResultSet` (`executor/types.rs:145`) carries only `columns`, `rows`, `notifications`. The work is: track created/deleted nodes, relationships, properties and labels through the write paths → surface them on `ResultSet` → assert them in the harness step (`tests/tck_runner.rs:190-197`, currently a documented no-op)
  **Bonus payoff:** `CypherResponse` (`nexus-server/src/api/cypher/mod.rs:173-191`) has **no `stats` field**, yet the project's own `CLAUDE.md` documents the `/cypher` response as returning `stats: { nodes_created, relationships_created, properties_set }`. That documented-but-missing field is the same data — implementing this closes both gaps at once. Wire it into the response envelope while you are here, and note it is additive (SDKs tolerate unknown fields)
  **Scope correction (2026-07-25):** the "counters do not exist" note above is now STALE. The scaffolding was since added: `SideEffects` (8 counters, TCK vocabulary) is defined at `executor/types.rs:161-185` and `ResultSet.side_effects` at `:204`, with a dedicated test `tests/executor/side_effects.rs`. Mechanism: a per-query accumulator — node/rel creation is counted via an `Arc<AtomicU64>` chokepoint on `RecordStore` (shared across `refresh_executor` clones), reset+read in `engine/query_pipeline.rs` (both entry points); other counters are meant to be incremented on the engine's `self.side_effects`. **ENGINE HALF DONE — all 8/8 counters wired on `ResultSet.side_effects`, TCK semantics, 19 tests in `tests/executor/side_effects.rs`, full executor harness (246) green:** `nodes_created`, `relationships_created`, `labels_added` (CREATE label bits), `properties_set` (inline CREATE map keys) via `Arc<AtomicU64>` storage chokepoints (`create_node`/`create_relationship`, summed with `+=` in `query_pipeline`); `nodes_deleted`, `relationships_deleted`, `labels_added`(SET n:L), `labels_removed`, `properties_set`(SET), `properties_removed` (REMOVE/SET null) via engine `self.side_effects` in `crud/nodes.rs` delete methods + `write_exec.rs` apply_set/remove, with idempotency (bit-flip / key-present) and no double-count. **Semantic decision RESOLVED (TCK-conformant): CREATE-d labels/properties DO count** — `types.rs` doc corrected and the CREATE test expectations updated to Neo4j-correct non-zero values.
  **`/cypher` `stats` field DONE** (`feat(server): surface query side-effects as /cypher stats`): `SideEffects` is `Serialize` + `is_empty()`; `CypherResponse.stats` is `skip_serializing_if` empty (read responses stay byte-identical — the 300-compat suite only compares `.rows`/`.columns`, verified at `test-neo4j-nexus-compatibility-200.ps1:141-170`); every builder threads it (success paths in `api/cypher/execute/handler.rs` emit the real `result(_set).side_effects`, error/empty/admin builders emit the zero default). Verified end-to-end on a live server: `CREATE (n:L {a,b})` → `stats {nodes_created:1, labels_added:1, properties_set:2}`; a read → no `stats` key; `DETACH DELETE` → `stats {nodes_deleted:1}`.
  **Harness `no side effects` assertion DONE** — `tck_runner.rs`'s no-op now asserts `ResultSet.side_effects.is_empty()`; validated against the spatial corpus (22 scenarios / 87 steps green, incl. `CREATE SPATIAL INDEX` DDL correctly counting zero). **STILL TODO (the one remaining piece, coupled to §3.2):** the `the side effects should be:` table step (`| +nodes | 1 |`, 244 corpus scenarios) — no scenario in the current spatial-only runner uses it, so add it in §3.2 when the runner drives the full vendored corpus and the assertion can be validated against real table scenarios. The counters + `/cypher stats` + the `no side effects` assertion are all in place.
- [ ] 2.2 Implement a real error taxonomy. `a <X> should be raised at runtime: <token>` (`:170-188`) ignores the error *kind* and substring-matches, because Nexus has no openCypher error classification (`:162-169`). Map Nexus errors onto the TCK's expected kinds so a scenario cannot pass by coincidentally containing a substring
- [ ] 2.3 Extend the table-cell parser (`:333-549`) to the upstream literal set: temporal and duration values, node/relationship/path literals, and map/list nesting as the TCK tables use them. It currently covers only what the 22 spatial scenarios need
- [ ] 2.4 Add an explicit, **counted and reported** skip-list for knowingly-unsupported categories. Skips must appear in the report as skips — silent omission is what makes a conformance number a lie

## 3. Vendor the TCK and take a baseline
- [x] 3.1 Vendor `tck/features/` from `opencypher/openCypher@677cbafabb8c3c5eed458fd3b1ec0daec8d67d23` into `crates/nexus-core/tests/tck/opencypher/` — 220 files, 1615 scenarios, 2.1 MB. Pin the commit in a README, add licence attribution (`LICENSE-NOTICE.md` is the existing precedent), and keep the Nexus-authored spatial corpus in its own directory untouched
- [ ] 3.2 Generalise the runner from the spatial-only `SpatialWorld` (`tck_runner.rs:34-43`) into a TCK runner over the vendored corpus, reusing the per-scenario isolated engine (`setup_isolated_test_engine`, `:94-100`). Preserve the Windows 8 MiB-stack `main()` workaround (`:561-598`)
- [ ] 3.3 Produce `docs/compatibility/OPENCYPHER_TCK_REPORT.md` with per-category pass/fail/skip counts and the pinned upstream commit, plus a reproducible entry point (`scripts/compatibility/run-opencypher-tck.ps1` or a cargo alias). **This number is the task's primary deliverable** — everything before it exists to make it trustworthy

## 4. Close the verified gaps
- [ ] 4.1 **Silent wrong-results bug:** `MATCH (n:$label)` parses (`parser/clauses/pattern.rs:506-522`, called for all node patterns at `:335`) but resolves nowhere on the read path, so it matches a *literal label named `$label`* instead of erroring (`engine/query_pipeline.rs:59-62`, sentinel handled only for CREATE at `:722-734`). Resolve the sentinel at execution start via catalog lookup. Note `WHERE n:$x` already works (`engine/tests/query.rs:888-893`) — mirror its semantics, including how it collapses NULL/empty/non-STRING to no rows
- [ ] 4.2 Regression test for 4.1 asserting the **wrong-results** behaviour specifically: a graph containing both a node labelled `Foo` and (if constructible) a node labelled literally `$label` must not be confused. A test that only checks "the right node is returned" would have passed before the fix in some graphs
- [ ] 4.3 Genuine parse gap: `parse_types` (`parser/clauses/pattern.rs:525-540`) has no `$` branch, so `-[r:$type]->` fails to parse. Add it, mirroring the label sentinel representation, then resolve it on the same path as 4.1
- [ ] 4.4 List-valued and invalid dynamic labels/types: fall back to AllNodesScan+Filter for LIST parameters; raise a typed error on non-STRING/LIST rather than silently matching nothing
- [ ] 4.5 `UNION` / `UNION ALL` inside `CALL { }`: this parses today (`parser/clauses/subquery.rs:88` → `parse_clause`, `clauses/mod.rs:284-287`); the gap is that `operators/call_subquery.rs` only inspects `Clause::Return` (`:134`, `:233`) and never `Clause::Union`. Wire the branches through the existing Union operator with per-branch scope validation (all branches must export identical columns)
- [ ] 4.6 **`SHOW INDEXES` is unimplemented** — `SHOW INDEXES` fails to parse: `Parse error: Cypher syntax error: SHOW must be followed by DATABASES, USERS, USER, FUNCTIONS, CONSTRAINTS, QUERIES, or API KEYS at line 1, column 6`. Neo4j implements it and it is the only supported way to introspect which indexes exist, so every tool that verifies its own schema prep is blind. The omission is inconsistent within Nexus itself: **`SHOW CONSTRAINTS` is fully implemented** and is the exact template to follow — parser branch `executor/parser/clauses/mod.rs:330-332`, AST variant `executor/parser/ast.rs:139`, dispatch `engine/ddl.rs:657`, and the DDL-routing predicate `engine/query_pipeline.rs:477`. Add the mirror-image `INDEXES` branch at `clauses/mod.rs:316-344` (and extend the error string at `:343`). The data already exists: the label/property pairs registered by `CREATE INDEX` are held in the catalog (`catalog/store.rs:82`, reloaded at startup) and composite indexes carry an optional user-supplied name (`index/composite_btree.rs:40`). Neo4j's result columns are `id, name, state, populationPercent, type, entityType, labelsOrTypes, properties, indexProvider`; at minimum emit `name, type, entityType, labelsOrTypes, properties`.
      **Repro** (Nexus 2.5.0, `POST /cypher`):
      ```
      CREATE INDEX snb_person_id IF NOT EXISTS FOR (n:Person) ON (n.id);  -- ok, returns ["Person.id.property"]
      SHOW INDEXES;                                                       -- Parse error (expected: one row per index)
      ```
      **Discovered by** `phase7_ldbc-snb-benchmark` item 1.2 (LDBC SNB schema prep): the benchmark's schema step can create its 15 property indexes but cannot assert afterwards that they exist, so it has to infer index coverage indirectly from the absence of `Nexus.Performance.UnindexedPropertyAccess` notifications. Closing this replaces that workaround with a direct assertion.

- [ ] 4.7 **Negative numeric literals are rejected in CREATE property maps.** `CREATE (:T {v: -7})` fails with `Cypher execution error: Complex expressions not supported in CREATE properties`, while `CREATE (:T {v: 7})` succeeds. Neo4j accepts both. A leading `-` parses as a unary-minus expression wrapping the literal rather than as a negative literal, so it falls through to the catch-all arm that rejects non-literal expressions. **Two call sites, both need the same fix**: `engine/match_exec.rs:573-575` and `executor/operators/create.rs:634-636` (the latter sits directly below the `Literal::Float`/`Boolean`/`Null` arms at `:617-626`, which is where the folded value belongs). Constant-fold unary minus over `Literal::Integer`/`Literal::Float` before the catch-all; consider folding constant arithmetic generally, but negative literals are the case that actually bites. Verify `SET n.v = -7` and relationship property maps (`CREATE (a)-[r:T {w: -1}]->(b)`) take the same path. **Repro**: `CREATE (:NegT {v: -7})` → error; `MATCH (n:NegT) RETURN n.v` → no rows. Discovered by `phase7_ldbc-snb-benchmark` item 1.2. LDBC ids are non-negative so the benchmark is not blocked, but any dataset with negative values cannot be loaded through inline CREATE maps.

- [x] 4.8 **CRITICAL — silent wrong results: expanding from a label scan loses rows NON-DETERMINISTICALLY on a large graph.** FIXED.
      **Root cause** (not where the note below first guessed): the executor decided "is this row value a node or a relationship?" with `obj.contains_key("type")` at 13 sites. `read_relationship_as_value_with_store` stores the relationship type under the key `type`, but `type` is ALSO an ordinary property name — LDBC's `Organisation.type` (company/university) and `Place.type` (city/country/continent) are exactly that. So every such NODE was misread as a relationship. The damage landed in `update_result_set_from_rows`' row-dedup key: the misidentified node was picked by `HashMap` iteration order, the real node variables were then excluded from the key, and unrelated rows collapsed into one — non-deterministically, because `HashMap` order varies run to run. The `find_relationships`/`first_rel_ptr` path the note suspected was a red herring; the storage was complete and the adjacency correct (the write counter and index seeks both proved it).
      **Fix**: a reserved structural marker `_nexus_rel_type`, written ONLY by the relationship constructor and never derivable from user data, plus `is_relationship_value` / `is_node_value` predicates in `executor/mod.rs`. All 13 `contains_key("type")` decision sites (helpers, projection core/fn_graph/fn_list, operators dispatch/filter/project, dispatch) now go through them. The `type` key is still emitted for `type(r)` and the Neo4j-shaped flat format, so no output changed except one additive internal field alongside the pre-existing `_nexus_id`.
      **Verified**: reproduced on the loaded LDBC SF0.1 graph, then confirmed fixed — `MATCH (o:Organisation)-[r:IS_LOCATED_IN]->(p:Place) RETURN count(r)` is now a stable 7955, and `ldbc-load --strict-readback` passes every per-type count. Regression tests in `tests/regression/node_property_named_type_test.rs` (a node with a `type` property is not deduplicated away, is stable across 10 runs, keeps its property readable, and a relationship whose property map contains `type` still survives) fail with the old `contains_key("type")` semantics and pass with the marker. Full nexus-core (2478) + nexus-server suites green.
      **Note (superseded by the fix above)** — original filing kept for the diagnostic trail:
      On the loaded LDBC SF0.1 graph (327 588 nodes / 1 492 038 relationships), a plain
      pattern count returns a DIFFERENT, always-short answer on every run of a read-only
      database. The rows are not missing from storage — they are missing from the traversal.
      **Repro** (Nexus 2.5.0, `POST /cypher`, after `benchmarks/ldbc-snb` loads SF0.1):
      ```
      MATCH (o:Organisation)-[r:IS_LOCATED_IN]->(p:Place) RETURN count(r)
        -> 5305, 5267, 5233   (three consecutive runs; the true answer is 7955)
      MATCH ()-[r:STUDY_AT]->() RETURN count(r)
        -> 893, 912, 895      (true answer 1209)
      MATCH (:Person)-[r:STUDY_AT]->(:Organisation) RETURN count(r)
        -> 1209, 1209, 1209   (correct and stable — same edges, smaller driving label)
      ```
      **Proof the data is present and complete** (same database, same session):
      ```
      MATCH (o:Organisation) WHERE NOT (o)-[:IS_LOCATED_IN]->() RETURN count(o)   -> 0
      MATCH (o:Organisation {id: 7954})-[:IS_LOCATED_IN]->(p:Place) RETURN p.id   -> 1449 (every sampled id resolves)
      MATCH (o:Organisation) RETURN count(o)                                      -> 7955 (stable)
      GET /stats -> catalog.rel_count = 1492038 = exactly what the loader submitted
      ```
      Every `count` variant of the failing query drifts (`count(*)`, `count(o)`,
      `count(DISTINCT o)`, and with the destination label dropped), so the instability is in
      the ROW SET the expand produces, not in aggregation. `count(DISTINCT o)` drifting means a
      different subset of source nodes is expanded on each run.
      **Ruled out** by synthetic reproduction attempts against a fresh server (all correct and
      stable): 2 000 / 4 000 / 5 000 / 8 000 sources fanning into 10 shared destinations, and
      the SNB write ordering where a node group accumulates 40 000 incoming edges before its
      own outgoing edge is created. Neither driving-row count nor incoming/outgoing ordering
      alone reproduces it — total graph size appears to be a factor.
      **Where to look**: `executor/operators/path.rs::find_relationships` is the only live
      lookup. It walks `first_rel_ptr`/`next_src_ptr` and silently SWALLOWS read failures
      (`if let Ok(rel_record) = store.read_rel(...)`), so an intermittently failing record read
      under page-cache pressure would present exactly as this: a varying subset, no error. Its
      `first_rel_ptr == 0` fallback also scans only the first 501 relationship ids, which cannot
      find an edge in a 1.5 M-edge store. Start by making both paths surface the error instead
      of dropping the row, then compare against the store's authoritative adjacency index
      (`storage::adjacency_index`, added by `phase0_perf-store-reverse-incoming-adjacency-index`),
      which knows every live edge of a node in both directions and is the natural replacement
      for the chain walk.
      **Discovered by** `phase7_ldbc-snb-benchmark` item 1.3, whose post-load count verification
      is what caught it. **This blocks the benchmark's query-correctness phase (1.4/1.5)**: no
      IS/IC query result can be validated against Neo4j while an identical query returns
      different answers on consecutive runs.

- [x] 4.9 **Multi-row `MATCH … CREATE` writes N² relationships instead of N.** FIXED. Found while
      writing the 4.8 regression fixture. `MATCH (a:A), (b:B) CREATE (a)-[:R]->(b)` over 3 A's and
      1 B created 9 edges, not 3 — always the SQUARE of the driving-row count.
      **Root cause**: by the time a CREATE runs, the read pipeline has already materialised its
      driving rows as ALIGNED columns (a comma-joined MATCH puts its cartesian product into
      `a=[a1,a2,a3]`, `b=[b1,b2,b3]`, index i = one row). `execute_create_with_context`'s slow path
      called `materialize_rows_from_variables`, which RE-CROSSES equal-length multi-element columns
      into N² — the exact defect `phase0_fix-materialize-recrosses-aligned-columns` fixed on the
      READ path (`seed_scan_main_loop` zips), but the CREATE path had its own materialisation and
      was missed. **Fix**: the slow path now calls `materialize_aligned_rows` (zip by index), one
      CREATE per driving row. `crates/nexus-core/src/executor/operators/create.rs:945`.
      **Verified**: `tests/regression/match_create_multi_pattern_test.rs` (3 A's × 1 B → 3 edges;
      5 P × 4 Q → 20 not 400; single-inline, UNWIND-create, and UNWIND-by-id paths stay correct)
      fails on the re-cross and passes with the zip. Full regression + engine + cypher + executor
      groups green. The LDBC loader was never affected (it writes via `/ingest`, not Cypher CREATE).

- [x] 4.10 **Multi-row `MATCH … MERGE (a)-[:T]->(b)` only processed the FIRST driving row.** FIXED.
      Found immediately after 4.9, while confirming the CREATE fix did not regress MERGE. Distinct
      subsystem (`engine/write_exec.rs::process_merge_relationship`, not the executor CREATE path).
      **Repro** (Nexus 2.5.0, fresh db): `CREATE (:C {id:1}), (:C {id:2}), (:D {id:8})` then
      `MATCH (c:C), (d:D) MERGE (c)-[:S]->(d)` → `MATCH ()-[r:S]->() RETURN count(r)` was 1, must be 2.
      **Root cause**: `process_merge_relationship` resolved each endpoint by collapsing its binding
      list to `ids[0]`, so when a preceding MATCH bound the endpoint to MULTIPLE nodes, every row
      after the first was silently dropped. Unlike the read path, `process_match_clause_multi` stores
      an INDEPENDENT id list per variable, so the correct driving set is the cartesian product of the
      two endpoint lists — exactly what the read-side relationship binder in the same function already
      iterates. **Fix**: the function now returns `Vec<(rel_var, rel_id, rel_type)>` and MERGEs the
      `src_ids × dst_ids` product, preserving every special case (bound-but-empty → node-only fallback,
      anonymous/standalone endpoint → `merge_single_node` single, direction swap, inline props, ON
      CREATE / ON MATCH per edge). Both call sites iterate the returned vec. The single-inline and
      per-row UNWIND paths bind one node per endpoint, so their product is one edge — unchanged.
      **Verified**: `tests/regression/match_merge_multi_pattern_test.rs` (once-per-row, idempotent,
      3×4 full cartesian, ON CREATE fires per edge, incoming-direction reversal per pair, single +
      UNWIND unchanged) fails under the old `ids[0]` semantics (5 of 6, single/UNWIND stays green,
      proving no regression) and passes with the cartesian. regression + cypher + database + storage
      + engine groups green. NOTE: the LDBC loader was never affected (it writes via `/ingest`).

- [x] 4.11a **CRITICAL — incoming/undirected relationship traversal returned nothing on graphs with
      more than ~10 000 relationships.** FIXED. Found while validating the LDBC short reads against
      Neo4j: `MATCH (:Person {id:933})<-[:HAS_CREATOR]-(:Message)` returned 0 on the loaded SF0.1
      graph while the reverse direction returned 412 and the edges were provably present.
      **Root cause**: `executor/operators/path.rs::find_relationships` served an INCOMING (or `Both`)
      expansion by checking the node's `first_rel_ptr` — which heads only its OUTGOING chain — finding
      it did not point at an incoming edge, and falling back to a scan that only ever probed
      relationship ids `0..=10_000`. Any incoming edge at a higher id was invisible. Small graphs
      worked (the scan covered them), which is why unit tests never caught it. **Fix**: repointed
      `find_relationships` at the store's authoritative both-direction adjacency index
      (`storage::adjacency_index`, from `phase0_perf-store-reverse-incoming-adjacency-index`) —
      O(degree), complete at any scale — replacing ~470 lines of chain walk + capped scan with one
      lookup. Verified: the failing LDBC query is now 412 = the Neo4j baseline, and six of seven
      short reads (IS1,3,4,5,6,7) match Neo4j on every sampled id. Regression:
      `tests/regression/incoming_traversal_large_graph_test.rs` (>10 000 edges + a late incoming edge;
      + reopen). Full lib + cypher + executor + storage + graph + regression green. Commit 88f78245.

- [ ] 4.11 **Variable-length path expanded from a `WITH`-carried variable does not bind its target.**
      The one remaining blocker of LDBC IS2. **Repro** (Nexus 2.5.0, loaded SF0.1):
      `MATCH (:Person {id:933})<-[:HAS_CREATOR]-(m:Message) WITH m LIMIT 3 MATCH (m)-[:REPLY_OF*0..]->(post:Post) RETURN m.id, post.id`
      → Nexus binds `m` but returns `post = null` for every row; Neo4j binds `post`. The FRESH-MATCH
      form (no `WITH` in between) binds `post` correctly, and a fixed-length expand after `WITH` also
      works — so the defect is specifically a VARIABLE-LENGTH expand whose source variable was carried
      across a `WITH … LIMIT`. IS1,3,4,5,6,7 validate against Neo4j; only IS2 needs this. The ported
      query is faithful (marked ⛔ in `benchmarks/ldbc-snb/README.md`); do not rewrite it to dodge the
      gap.

- [ ] 4.12 **`count(*)` / anonymous-relationship traversal under-counts a fully-anonymous pattern.**
      Lower severity (no IS query hits it — they all bind the relationship or a labelled endpoint).
      **Repro** (loaded SF0.1): `MATCH ()-[:HAS_CREATOR]->() RETURN count(*)` = 1461, but
      `MATCH ()-[r:HAS_CREATOR]->() RETURN count(r)` = 286744 (correct), and
      `MATCH (:Message)-[:HAS_CREATOR]->(:Person) RETURN count(*)` = 286744 (correct). Likewise
      `MATCH (p:Person {id:933})-[:KNOWS]-(f) RETURN count(f)` = 3 but `-[r:KNOWS]-` = 6. So a
      fully-anonymous relationship pattern (no rel variable, both endpoints unlabelled) collapses the
      match count. Binding the relationship or labelling an endpoint both fix it, which is why it does
      not surface in the reference queries — filed for completeness, not blocking the benchmark.

## 5. Re-measure and reconcile the documentation
- [ ] 5.1 Re-run the TCK after §4 and refresh `docs/compatibility/OPENCYPHER_TCK_REPORT.md`; the delta from the §3.3 baseline is the evidence that §4 mattered
- [ ] 5.2 Reconcile the compatibility claim, which currently spans 40 points across six files, to the single measured number: `AGENTS.override.md:159` (~55%), `docs/PRD.md:24`, `docs/ROADMAP.md:6`, `docs/guides/USER_GUIDE.md:26`, `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md:84` ("toward ~95%"), `docs/nexus/README.md:22` ("~85%"). State plainly what is measured (TCK pass rate) versus what is a differential result (the 325-case Neo4j suite) — conflating them is how the spread arose. **Do NOT edit `CLAUDE.md`**: it is generated between `RULEBOOK:START/END` sentinels, marked DO NOT EDIT BY HAND at `:1-3`, and does not mention openCypher

## 6. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 6.1 Update or create documentation covering the implementation (`docs/specs/cypher-subset.md` for dynamic labels/types, CALL+UNION, and the `FOR … REQUIRE` form from 1.1; the TCK report and how to run it; CHANGELOG entry)
- [ ] 6.2 Write tests covering the new behavior (unit tests per parser/planner change, written 1–3 at a time and run immediately; plus TCK scenarios exercising each fixed feature)
- [ ] 6.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace` green, TCK runner green on all non-skipped categories with the skip count reported)

## Related (verified shipped — do NOT treat as blockers)
All four are in `.rulebook/archive/` with every item checked: `phase6_opencypher-quantified-path-patterns`,
`phase6_opencypher-subquery-transactions`, `phase6_opencypher-geospatial-predicates`,
`phase7_planner-using-index-hints`.
