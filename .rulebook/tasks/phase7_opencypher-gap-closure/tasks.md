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
- [x] 2.1 Implement real side-effect assertions. **Measured against the vendored corpus: 1498 of 1615 scenarios (93%) assert side effects** — 1180 use `And no side effects`, 244 use `And the side effects should be:` with tables like `| +nodes | 1 |`. Skipping them is therefore not an option; it would gut the corpus. The counters the TCK expects are `+nodes`/`-nodes`, `+relationships`/`-relationships`, `+properties`/`-properties`, `+labels`/`-labels`
  **Scope discovery — this is an engine feature, not a harness tweak.** Side-effect counters do not exist anywhere in the codebase: `grep nodes_created` finds nothing in `nexus-core` or `nexus-server`, and `ResultSet` (`executor/types.rs:145`) carries only `columns`, `rows`, `notifications`. The work is: track created/deleted nodes, relationships, properties and labels through the write paths → surface them on `ResultSet` → assert them in the harness step (`tests/tck_runner.rs:190-197`, currently a documented no-op)
  **Bonus payoff:** `CypherResponse` (`nexus-server/src/api/cypher/mod.rs:173-191`) has **no `stats` field**, yet the project's own `CLAUDE.md` documents the `/cypher` response as returning `stats: { nodes_created, relationships_created, properties_set }`. That documented-but-missing field is the same data — implementing this closes both gaps at once. Wire it into the response envelope while you are here, and note it is additive (SDKs tolerate unknown fields)
  **Scope correction (2026-07-25):** the "counters do not exist" note above is now STALE. The scaffolding was since added: `SideEffects` (8 counters, TCK vocabulary) is defined at `executor/types.rs:161-185` and `ResultSet.side_effects` at `:204`, with a dedicated test `tests/executor/side_effects.rs`. Mechanism: a per-query accumulator — node/rel creation is counted via an `Arc<AtomicU64>` chokepoint on `RecordStore` (shared across `refresh_executor` clones), reset+read in `engine/query_pipeline.rs` (both entry points); other counters are meant to be incremented on the engine's `self.side_effects`. **ENGINE HALF DONE — all 8/8 counters wired on `ResultSet.side_effects`, TCK semantics, 19 tests in `tests/executor/side_effects.rs`, full executor harness (246) green:** `nodes_created`, `relationships_created`, `labels_added` (CREATE label bits), `properties_set` (inline CREATE map keys) via `Arc<AtomicU64>` storage chokepoints (`create_node`/`create_relationship`, summed with `+=` in `query_pipeline`); `nodes_deleted`, `relationships_deleted`, `labels_added`(SET n:L), `labels_removed`, `properties_set`(SET), `properties_removed` (REMOVE/SET null) via engine `self.side_effects` in `crud/nodes.rs` delete methods + `write_exec.rs` apply_set/remove, with idempotency (bit-flip / key-present) and no double-count. **Semantic decision RESOLVED (TCK-conformant): CREATE-d labels/properties DO count** — `types.rs` doc corrected and the CREATE test expectations updated to Neo4j-correct non-zero values.
  **`/cypher` `stats` field DONE** (`feat(server): surface query side-effects as /cypher stats`): `SideEffects` is `Serialize` + `is_empty()`; `CypherResponse.stats` is `skip_serializing_if` empty (read responses stay byte-identical — the 300-compat suite only compares `.rows`/`.columns`, verified at `test-neo4j-nexus-compatibility-200.ps1:141-170`); every builder threads it (success paths in `api/cypher/execute/handler.rs` emit the real `result(_set).side_effects`, error/empty/admin builders emit the zero default). Verified end-to-end on a live server: `CREATE (n:L {a,b})` → `stats {nodes_created:1, labels_added:1, properties_set:2}`; a read → no `stats` key; `DETACH DELETE` → `stats {nodes_deleted:1}`.
  **Harness `no side effects` assertion DONE** — `tck_runner.rs`'s no-op now asserts `ResultSet.side_effects.is_empty()`; validated against the spatial corpus (22 scenarios / 87 steps green, incl. `CREATE SPATIAL INDEX` DDL correctly counting zero). **`the side effects should be:` table step DONE** (commit `1c70d066`, in the vendored-corpus runner `tests/tck_opencypher.rs`): `side_effects_should_be` parses the `| +nodes | 1 |` table into an expected `SideEffects` (unlisted counters stay zero — the TCK table is exhaustive; an unknown key is a hard error) and asserts `result.side_effects == expected`. Validated against the real corpus (it is one of the active assertions in the 3868-scenario baseline; 20 of the fails are genuine side-effect mismatches). The counters + `/cypher stats` + `no side effects` + the table step are all in place — §2.1 fully complete.
- [x] 2.2 Implement a real error taxonomy. `a <X> should be raised at runtime: <token>` (`:170-188`) ignores the error *kind* and substring-matches, because Nexus has no openCypher error classification (`:162-169`). Map Nexus errors onto the TCK's expected kinds so a scenario cannot pass by coincidentally containing a substring
  **DONE (2026-07-25, commit `363557b6`).** Engine: `OpenCypherErrorKind` (the 8 TCK kinds — SyntaxError, SemanticError, TypeError, ArgumentError, EntityNotFound, ConstraintVerificationFailed, ParameterMissing, ProcedureError — plus an honest `Uncategorized`) and `Error::opencypher_kind()` in `crates/nexus-core/src/error.rs`, re-exported at `lib.rs`. **Classification is code-first**, which the corpus forced: the same logical error is emitted under DIFFERENT Rust variants at different call sites (`ERR_CRS_MISMATCH` is `CypherSyntax` in `eval/projection/fn_geo.rs` but `CypherExecution` in `spatial/mod.rs`), so the coarse variant cannot decide the kind. A structured leading `ERR_*` code prefix (`structured_code()`) is matched first via a curated `from_error_code` map (spatial + arg/param/proc/constraint codes), then a variant fallback (`CypherSyntax`→SyntaxError, `TypeMismatch`→TypeError, `ConstraintViolation`→ConstraintVerificationFailed, `NotFound`/`InvalidId`→EntityNotFound, `InvalidInput`→ArgumentError), else `Uncategorized`. 11 unit tests (code-override-wins, cross-variant, honest fallback, leading-vs-trailing-code). Harness: `tck_runner.rs`'s error step now captures the classified kind from the typed error in `try_run_cypher` (`last_error_kind`) and **asserts the kind equals the feature's `<X>`** (`ConstraintError` accepted as an alias of `ConstraintVerificationFailed`), keeping the detail-token substring check as a secondary assertion; the step regex now binds BOTH `compile time` and `runtime` phases so the full corpus maps to a step (phase captured but not asserted — Nexus detects some statically-provable errors only at execution time, and failing on that would be a false negative, not a real gap). Validated against the spatial corpus: 22 scenarios / 87 steps green with the kind assertion active (`ERR_CRS_MISMATCH`→TypeError ×2, `ERR_RTREE_BUILD`→ConstraintVerificationFailed ×1). **Coupled to §3.2:** broadening the `from_error_code` map + variant classification to the vendored corpus's standard tokens (`InvalidArgumentType`, `UndefinedVariable`, …) is iterative — it can only be driven by seeing which real scenarios fail once the full-corpus runner exists; the taxonomy mechanism and the honest `Uncategorized` default are in place so that measurement is trustworthy rather than substring-inflated.
- [x] 2.3 Extend the table-cell parser (`:333-549`) to the upstream literal set: temporal and duration values, node/relationship/path literals, and map/list nesting as the TCK tables use them. It currently covers only what the 22 spatial scenarios need
  **DONE (2026-07-25).** Node/relationship/path literals and nested map/list were already implemented (the parser moved to `tests/tck_common/mod.rs`, `@tck_node`/`@tck_rel`/`@tck_path` markers + structural comparison). **Definitive corpus audit** (classified every "result should be" value cell across all 220 files / 1615 scenarios against the parser grammar): the ONLY result-table literal the parser rejected was the unquoted IEEE special `NaN` (3 cells — `ReturnOrderBy1` [11]/[12], `WithOrderBy1`, all from `0.0 / 0.0`). **Temporal and duration values are NOT a distinct parse target** — the corpus renders every temporal/duration RESULT as a quoted string (`'2015-07-21'`, `'1816-12-23T00:00'`, `'P14DT16H12M'`, `'PT-23H-59M-59.9S'`); the `date(...)`/`duration({...})` forms appear only in Examples INPUT columns (interpolated into the query, never parsed as a result cell). So they already parse as `Value::String` and any residual temporal fails are ENGINE gaps (stringly-typed temporal, kept as real fails per §2.4), not harness parse gaps. **NaN is currently unreachable** — Nexus converts every non-finite arithmetic result into an error (`eval/arithmetic.rs:171` div-by-zero, `eval/projection/fn_math.rs:134` "non-finite", and `serde_json::Number::from_f64` cannot hold NaN/±∞, confirmed by the codebase's own `executor/mod.rs:198` test helper), so a NaN-producing query errors before the result table is ever compared. **Implemented anyway (defensive, so §5.1's re-measure never panics on the cell and measures it correctly should the engine gain openCypher non-finite semantics):** the parser now accepts `NaN`/`Infinity`/`-Infinity` as a `@tck_float` marker (JSON `Number` can't carry them); `values_equal` routes the marker to `tck_float_matches`, which does the numeric `is_nan`/`is_infinite` check — it correctly matches nothing in Nexus today (an attributable FAIL, not a panic). Temporal/duration string-rendering is now documented in the `tck_cell_to_json` doc-comment so the item is not falsely reopened. 7 new unit tests in `tests/tck_cells.rs` (20/20 green — IEEE parse incl. list-nested, temporal/duration-as-string lock, NaN-vs-finite/null/string non-match); `cargo +nightly fmt --check` clean, `clippy --tests -D warnings` clean, both `harness=false` corpus runners compile.
- [x] 2.4 Add an explicit, **counted and reported** skip-list for knowingly-unsupported categories. Skips must appear in the report as skips — silent omission is what makes a conformance number a lie
  **DONE (commit `f924bfb6`).** A `filter_run` predicate skips scenarios the runner cannot EVALUATE (not scenarios Nexus gets wrong) and counts each by reason, rendered in a "Skip-list" section of the report: 164 deliberate skips — query parameters not wired (62), procedure registration (50), control-query references (33), named fixture graphs binary-tree-N (19). This de-inflated the fail column by 27 (spurious harness-limitation fails → counted skips): 3868 scenarios → 506 pass / 3178 fail / 184 skip. **Deliberately NOT skip-listed: temporal** (and anything Nexus attempts but gets wrong) — those stay as real fails, since hiding a real gap behind a skip is exactly the lie this item guards against.

## 3. Vendor the TCK and take a baseline
- [x] 3.1 Vendor `tck/features/` from `opencypher/openCypher@677cbafabb8c3c5eed458fd3b1ec0daec8d67d23` into `crates/nexus-core/tests/tck/opencypher/` — 220 files, 1615 scenarios, 2.1 MB. Pin the commit in a README, add licence attribution (`LICENSE-NOTICE.md` is the existing precedent), and keep the Nexus-authored spatial corpus in its own directory untouched
- [x] 3.2 Generalise the runner from the spatial-only `SpatialWorld` (`tck_runner.rs:34-43`) into a TCK runner over the vendored corpus, reusing the per-scenario isolated engine (`setup_isolated_test_engine`, `:94-100`). Preserve the Windows 8 MiB-stack `main()` workaround (`:561-598`)
  **DONE (commit `1c70d066`).** New `harness = false` binary `tests/tck_opencypher.rs` (its own Cargo `[[test]]`), reusing the shared `tck_common` comparison helpers (extracted in `968b368b`) and the 8 MiB-stack + `Opts::default()` main. A generic `TckWorld` mirrors `SpatialWorld` (isolated engine per scenario) and defines the corpus's generic steps: `an empty graph`/`any graph`, `having executed:`, `executing query:`, `the result should be, in any order:`/`in order:`/`empty`, `no side effects`, `the side effects should be:` (the §2.1 tail), and the error step (kind assertion, `compile time`/`runtime`/`any time`, `*` wildcard). Per-category pass/fail/skip is tallied via cucumber's `after` hook into a static map; unmatched steps land as `StepSkipped`, so scenarios using a not-yet-supported step (`parameters are:`, `there exists a procedure`, `binary-tree`, control query) are skips, never inflating pass/fail. Gated behind `NEXUS_TCK=1` so a plain `cargo test` skips the ~3800-scenario run. The 1 corpus file `Match5.feature` fails to parse under gherkin 0.14 (noted; 1 file).
- [x] 3.3 Produce `docs/compatibility/OPENCYPHER_TCK_REPORT.md` with per-category pass/fail/skip counts and the pinned upstream commit, plus a reproducible entry point (`scripts/compatibility/run-opencypher-tck.ps1` or a cargo alias). **This number is the task's primary deliverable** — everything before it exists to make it trustworthy
  **FIRST BASELINE DONE (commit `1c70d066`).** The runner writes the report (per-category table + totals + pinned commit + reproduce command); `scripts/compatibility/run-opencypher-tck.ps1` is the entry point. **Baseline: 3868 scenarios (1615 outline-expanded) — 506 pass (13.1%), 3204 fail, 158 skip**, all 37 categories present. The strict TCK gives no partial credit, so this sits far below the differential Neo4j figure by design. Strong: `conditional` 100%, `null` 77%, `union`/`mathematical` 50%. Fail column dominated by `temporal` (1004 scenarios, engine is stringly-typed — §2.3/§2.4) and `quantifier` (604). **To refine into the trustworthy number: §2.3** turns ~150 graph-element-cell parse-fails into attributable real results, **§4.13** (node labels) unblocks a large share of node-scenario fails, **§2.4** moves knowingly-unsupported to counted skips, then **§5.1** re-measures the delta.

## 4. Close the verified gaps
- [x] 4.1 **Silent wrong-results bug:** `MATCH (n:$label)` parses (`parser/clauses/pattern.rs:506-522`, called for all node patterns at `:335`) but resolves nowhere on the read path, so it matches a *literal label named `$label`* instead of erroring (`engine/query_pipeline.rs:59-62`, sentinel handled only for CREATE at `:722-734`). Resolve the sentinel at execution start via catalog lookup. Note `WHERE n:$x` already works (`engine/tests/query.rs:888-893`) — mirror its semantics, including how it collapses NULL/empty/non-STRING to no rows
  **DONE (2026-07-25).** The scan site was pinned to the planner's pattern-lowering `plan_execution_strategy` (`executor/planner/queries/strategy.rs`), at BOTH the primary start-pattern scan (`~:173`) and the additional comma-separated pattern loop (`~:434`): each resolved `node.labels[0]` via `self.catalog.get_or_create_label(first_label)?` at PLAN time — where query params are NOT available (they live on the execution context, not the `QueryPlanner`) — so a `$label` sentinel matched/created a literal label. **Fix (deferral, not plan-time resolution):** when `first_label.starts_with('$')`, emit `AllNodesScan` + `Filter("{variable}:{first_label}")` — the exact lowering already used for ADDITIONAL labels — instead of `get_or_create_label`; `operators/filter.rs:43-56` then resolves the sentinel against `context.params` at execution (non-empty STRING → label; NULL/missing/empty/non-STRING → no rows, mirroring `WHERE n:$x`; never an error, never the catalog write). The shared additional-labels loop is untouched, so `MATCH (n:$a:$b)` resolves both. No engine/execution changes — the operators and their runtime resolution already existed; only the planner branch is new. Verified: full `regression` group 161/0, `cypher` 406/0, `executor` 246/0, clippy `-D warnings` clean, fmt clean.
  **INVESTIGATION (2026-07-25, not yet fixed — two implementer passes truncated on navigation; recorded so a focused pickup doesn't restart from zero):**
  - **Bug confirmed at code level.** The read path passes a node pattern's raw labels (which may contain the `$param` sentinel string, e.g. `"$label"`) straight into the label scan without resolution; the write path (CREATE/SET/REMOVE) resolves via `self.resolve_dynamic_labels(&node.labels)` but the read path does not.
  - **The EXACT read mirror is `executor/operators/filter.rs:43-56`** (the `WHERE n:$x` path): `label_name.strip_prefix('$')` → `context.params.get(name)`; a **non-empty STRING** becomes the label, **anything else (NULL/missing/empty/non-STRING) yields an empty result set (no rows), NOT an error**. The MATCH scan must mirror THIS, not `engine::dynamic_labels::resolve_labels` — that helper raises `ERR_INVALID_LABEL` on NULL/non-STRING (correct for writes, wrong for reads). Parameters for the current query are on `self.current_params`.
  - **CAUTION — the scan site is NOT yet pinned.** A `grep node.labels.clone()` in `engine/match_exec.rs` is misleading: the hits around `:267/:274` are the CREATE loop (`create_node_with_transaction`), NOT the read scan. The actual MATCH label→scan resolution site (where a `(n:Label)` pattern drives a label-bitmap/AllNodesScan) still needs locating — most likely in the `executor/operators/` scan operators or the planner, not the `match_exec.rs` CREATE path. Pin it first, then apply the filter.rs mirror. `:335` in `parser/clauses/pattern.rs` and the `query_pipeline.rs` line refs in this item are STALE (code drifted).
  - **Scope note:** LIST-valued `$param` is §4.4 (AllNodesScan+Filter fan-out); for 4.1, a LIST/other value collapses to no-rows like any non-STRING.
- [x] 4.2 Regression test for 4.1 asserting the **wrong-results** behaviour specifically: a graph containing both a node labelled `Foo` and (if constructible) a node labelled literally `$label` must not be confused. A test that only checks "the right node is returned" would have passed before the fix in some graphs
  **DONE (2026-07-25, `tests/regression/dynamic_label_read_path_test.rs`, 4 tests, registered in `tests/regression/main.rs`).** Isolated per-test engine (`Engine::with_isolated_catalog` — avoids the shared-process-catalog flake). The positive discriminator (`MATCH (n:$label)` with `label='Foo'` → exactly `[1]`, and `'Bar'` → `[2]`) FAILS pre-fix (returns `[]` — the literal-`$label` scan) and PASSES post-fix — verified by stashing ONLY the strategy.rs hunk (2 of 4 fail pre-fix, all 4 pass restored). Plus: non-existent label → no rows; missing/NULL/empty/non-string param each collapse to no rows (mirrors `WHERE n:$x`); confusion guard asserts `label='Foo'` returns exactly the Foo node, not every node. Documented in-file: a literally-`$label`-labelled node is NOT constructible (the CREATE write path also resolves `$label` against the bound param via `resolve_dynamic_labels`), so cases 1-3 are the discriminator.
- [ ] 4.3 Genuine parse gap: `parse_types` (`parser/clauses/pattern.rs:525-540`) has no `$` branch, so `-[r:$type]->` fails to parse. Add it, mirroring the label sentinel representation, then resolve it on the same path as 4.1
  **SCOPED (2026-07-25, not yet implemented — recorded so a focused pickup starts from zero). VERDICT: this is NOT a clean mirror of §4.1 — it requires BUILDING new runtime `$type` resolution.** §4.1 worked only because the Filter operator ALREADY resolved a `$label` predicate against `context.params`; relationship types have no such free runtime resolver.
  - **Parser (small):** `parse_types` at `parser/clauses/pattern.rs:525-546` accepts only `:Ident`/`|Ident` — NO `$` branch (a `$` is not a valid identifier start, so `-[r:$type]->` fails at `parse_identifier`). Mirror the SIBLING `parse_labels` at `pattern.rs:511-514` (`if peek=='$' { consume; let p=parse_identifier()?; push(format!("${p}")) }`) in BOTH the first-type and the `|`-alternation loop. AST field is `RelationshipPattern.types: Vec<String>` (`parser/ast.rs:519`, struct at `:515`) — store `"$type"` verbatim.
  - **Resolution is PLAN-time today, and the planner has NO params.** Static `-[r:KNOWS]->` resolves `rel.types` → `type_ids: Vec<u32>` in the planner at `planner/queries/relationships.rs:87-99` (`get_type_id` → else `get_or_create_type`) and `planner/queries/qpp.rs:211-221`, baked into `Operator::Expand`/`VariableLengthPath`/`QppHopSpec`. A `$type` sentinel would hit `get_or_create_type("$type")` → registers a garbage type matching 0 edges. Execution operators receive only `type_ids: &[u32]` (`operators/expand.rs:24`, `operators/path.rs:37/455`) — no names, no `$` awareness.
  - **No existing runtime `$type` resolver anywhere.** Every `strip_prefix('$')`/`starts_with('$')` in the tree is for node LABELS (`engine/dynamic_labels.rs:40/141`, `operators/filter.rs:43`, `planner/queries/strategy.rs:177/462`). There is NO `resolve_dynamic_types`; the write-path dynamic-CREATE explicitly scopes itself to node labels only (`engine/match_exec.rs:858-861`). The label Filter is NOT reusable — its `"{var}:{label}"` predicate checks the node's label BITMAP (`filter.rs:73-74`), not a rel type.
  - **params ARE reachable one level up:** `ExecutionContext.params` (`context.rs:53`) at `execute_expand`/`execute_variable_length_path`, and `self.current_params` (`engine/mod.rs:204`) in the engine pipeline BEFORE planning (`query_pipeline.rs:74/135`).
  - **Recommended shape — option (b), pre-plan AST rewrite (smaller, mirrors the write-path pattern):** add a `resolve_types` resolver (sibling of `resolve_dynamic_labels`, `engine/mod.rs:941`) and rewrite each `RelationshipPattern.types` `$`-sentinel to its concrete name on the cloned AST against `self.current_params` before planning (hook near `has_dynamic_labels` dispatch, `query_pipeline.rs:796-817`); then the existing plan-time `get_type_id` "just works." Touches: parser `$` branch + one resolver + one AST-walk hook (+ verify `relationships.rs:87`/`qpp.rs:211` never see a raw `$type`). Option (a) (resolve inside the Expand/VarLength/QPP operators via `context.params`) is larger (operator/spec field + planner deferral + 3 exec sites).
  - **OPEN DESIGN DECISION (the crux — decide before implementing):** degenerate `$type` (NULL / missing / empty / non-STRING). §4.1's LABEL read path chose **no-rows** (via `filter.rs`); §4.4 explicitly owns "raise a typed error on non-STRING/LIST" + "LIST → AllNodesScan+Filter fan-out" for BOTH labels and types. So §4.3 = the single-STRING happy path + parsing; the no-rows-vs-typed-error policy and LIST handling belong to §4.4 and must NOT pollute the catalog (the garbage-type trap above). Coupled to §4.4 — consider doing them together.
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

- [x] 4.13 **`RETURN n` omits node labels from the returned value.** DONE (commit `bc661fbe`) — emit
      them under the reserved `_nexus_labels` key (matching the sibling `Engine::node_to_result_value`,
      which already did; additive like `_nexus_id`). **Honest outcome: NO conformance gain (net -1 noise,
      3868 → 505 pass).** The hypothesis that missing labels capped node scenarios was WRONG — with labels
      correct, node-returning scenarios still fail on DEEPER bugs surfaced by the baseline, filed below as
      4.14/4.15. Kept as a correctness/consistency fix (nodes now carry their labels; comparison is now
      accurate). Blast radius clean (nexus-core lib 2519/0, integration + server green modulo a pre-existing
      Windows/Tantivy fulltext flake). Original filing:
      Discovered by the §3.2 TCK
      baseline: `read_node_as_value_with_store` (`executor/operators/path.rs:1040`) computes the node's
      label names at `:1051-1054` into a `let _labels` binding that is then DISCARDED — the returned
      Value is a flat object of `{ <properties>, "_nexus_id": id }` with no labels key at all. openCypher
      (and Neo4j) treat labels as part of a node's identity, and the TCK result tables render nodes as
      `(:A {name: 'b'})`, so any scenario that returns a labelled node cannot match Nexus's output — the
      label has no counterpart. Relationships do NOT have this problem: `read_relationship_as_value_with_store`
      (`eval/helpers.rs:1076`) emits the type under both `_nexus_rel_type` and `type`. **Impact:** a large
      share of `clauses/match`, `clauses/create`, and `expressions/graph` fails in the baseline are this
      single gap. **Caution:** the flat `{props, _nexus_id}` node shape is deliberate for the Neo4j-array
      row format (`CLAUDE.md` "NEVER modify server response formats"), so adding a `_labels` key must be
      done as an ADDITIVE internal field (like `_nexus_id`) that the TCK comparison can read, mirrored on
      the relationship's `_nexus_rel_type` precedent — not by changing the existing property projection.

- [ ] 4.14 **Multi-variable node RETURN drops a binding to `Null`.** Surfaced by the §3.2 baseline, NOT yet
      root-caused. Several `clauses/match` scenarios that bind two node variables and `RETURN a, b` come back
      with the second column `Null` where a node was expected — e.g. a two-column result row materialised as
      `[<node>, Null]` against an expected `[<node>, <node>]`. This is the single largest attributable
      failure family in the node categories after 4.13. Root-cause the binding loss (likely the same
      alignment/partial-binding territory as the `expand-required-partial-binding-leak` note) before fixing;
      add a regression test that asserts BOTH columns bind. Reproduce from the failing scenarios the runner
      now reports (they were previously masked as cell-parse panics before §2.3).
- [ ] 4.15 **A labelled node scan appears to return unlabelled/other-labelled nodes.** Also surfaced by the
      baseline, NOT yet root-caused (could be a genuine label-filter gap or a mis-ported scenario — confirm
      the actual query first). Observed shape: an expected single `(:A)` row against a result carrying
      `(:A)`, `(:B {..})`, and `({..})` rows. If a real label-filter bug, it is high severity; if the
      scenario is `MATCH (n) RETURN n` with a differently-shaped expectation, it belongs with the row-set
      differences instead. Diagnose against the specific `clauses/match` scenario before filing a fix.

## 5. Re-measure and reconcile the documentation
- [ ] 5.1 Re-run the TCK after §4 and refresh `docs/compatibility/OPENCYPHER_TCK_REPORT.md`; the delta from the §3.3 baseline is the evidence that §4 mattered
- [x] 5.2 Reconcile the compatibility claim, which currently spans 40 points across six files, to the single measured number: `AGENTS.override.md:159` (~55%), `docs/PRD.md:24`, `docs/ROADMAP.md:6`, `docs/guides/USER_GUIDE.md:26`, `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md:84` ("toward ~95%"), `docs/nexus/README.md:22` ("~85%"). State plainly what is measured (TCK pass rate) versus what is a differential result (the 325-case Neo4j suite) — conflating them is how the spread arose. **Do NOT edit `CLAUDE.md`**: it is generated between `RULEBOOK:START/END` sentinels, marked DO NOT EDIT BY HAND at `:1-3`, and does not mention openCypher
  **DONE (commit `7f94f373`).** All six files reconciled to THREE clearly-separated metrics: (1) openCypher TCK conformance — the measured strict pass rate, first baseline ~13% (505/3868), referenced via `OPENCYPHER_TCK_REPORT.md` rather than frozen so §4/§5.1 can move it; (2) Neo4j differential — 300/300 on the 325-case suite, labelled as agreement-with-one-implementation not spec conformance; (3) feature-parity estimates (~85%/~95%) — labelled as code-verified feature-inventory coverage, distinct from both. `CLAUDE.md` NOT touched. **NOTE:** the number cited is the §3.3 baseline; when §4 gap-fixes land, §5.1 refreshes the report and these docs automatically track it (they point to the report, not a frozen %).

## 6. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 6.1 Update or create documentation covering the implementation (`docs/specs/cypher-subset.md` for dynamic labels/types, CALL+UNION, and the `FOR … REQUIRE` form from 1.1; the TCK report and how to run it; CHANGELOG entry)
- [ ] 6.2 Write tests covering the new behavior (unit tests per parser/planner change, written 1–3 at a time and run immediately; plus TCK scenarios exercising each fixed feature)
- [ ] 6.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace` green, TCK runner green on all non-skipped categories with the skip count reported)

## Related (verified shipped — do NOT treat as blockers)
All four are in `.rulebook/archive/` with every item checked: `phase6_opencypher-quantified-path-patterns`,
`phase6_opencypher-subquery-transactions`, `phase6_opencypher-geospatial-predicates`,
`phase7_planner-using-index-hints`.
