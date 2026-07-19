# phase7_opencypher-gap-closure — tasks

> **Revised 2026-07-19** after verifying the original draft. One item was refuted and
> removed (`FOR … REQUIRE` already ships), two were misdiagnosed as parser gaps, and
> the harness prerequisites were promoted ahead of the baseline — a pass rate measured
> with vacuous assertions would be worse than no number at all.

Order matters and is deliberate: make the harness honest, measure a baseline, then fix
gaps, then re-measure so the delta is attributable. Do not reorder §2 before §1.

## 1. Correct the record (cheap, immediate, no code)
- [ ] 1.1 Document the shipped-but-undocumented `CREATE CONSTRAINT … FOR (n:L) REQUIRE …` syntax in `docs/specs/cypher-subset.md` — it is fully implemented (`executor/parser/clauses/admin.rs:311-324`, body parser `:413-437`, tests `parser/tests/ddl.rs:186,205,222,239,256`) but absent from the spec, which is what led the original draft to believe it was missing
- [ ] 1.2 Fix the stale header in `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` (`:1`, `:21` say "300 Tests"; the file has **325** `Run-Test` calls), and decide the fate of the `.sh` sibling, which has drifted to 196 cases — reconcile it or delete it, but do not leave two suites claiming to be the same thing
- [ ] 1.3 Fix the stale `"status": "pending"` in `.rulebook/archive/2026-04-27-phase6_opencypher-subquery-transactions/.metadata.json` (it is archived with all 52 items done)

## 2. Make the harness honest (prerequisite — a baseline measured before this is inflated)
- [ ] 2.1 Implement real side-effect assertions. `no side effects` (`tests/tck_runner.rs:190-197`) is currently a **no-op stub**, so every TCK scenario asserting on side effects would pass vacuously. This needs `ResultSet` (or an adjacent channel) to expose created/deleted node/relationship/property/label counts, then the step must assert them — including the `+nodes`/`-nodes` table form the upstream TCK uses
- [ ] 2.2 Implement a real error taxonomy. `a <X> should be raised at runtime: <token>` (`:170-188`) ignores the error *kind* and substring-matches, because Nexus has no openCypher error classification (`:162-169`). Map Nexus errors onto the TCK's expected kinds so a scenario cannot pass by coincidentally containing a substring
- [ ] 2.3 Extend the table-cell parser (`:333-549`) to the upstream literal set: temporal and duration values, node/relationship/path literals, and map/list nesting as the TCK tables use them. It currently covers only what the 22 spatial scenarios need
- [ ] 2.4 Add an explicit, **counted and reported** skip-list for knowingly-unsupported categories. Skips must appear in the report as skips — silent omission is what makes a conformance number a lie

## 3. Vendor the TCK and take a baseline
- [ ] 3.1 Vendor `tck/features/` from `opencypher/openCypher@677cbafabb8c3c5eed458fd3b1ec0daec8d67d23` into `crates/nexus-core/tests/tck/opencypher/` — 220 files, 1615 scenarios, 2.1 MB. Pin the commit in a README, add licence attribution (`LICENSE-NOTICE.md` is the existing precedent), and keep the Nexus-authored spatial corpus in its own directory untouched
- [ ] 3.2 Generalise the runner from the spatial-only `SpatialWorld` (`tck_runner.rs:34-43`) into a TCK runner over the vendored corpus, reusing the per-scenario isolated engine (`setup_isolated_test_engine`, `:94-100`). Preserve the Windows 8 MiB-stack `main()` workaround (`:561-598`)
- [ ] 3.3 Produce `docs/compatibility/OPENCYPHER_TCK_REPORT.md` with per-category pass/fail/skip counts and the pinned upstream commit, plus a reproducible entry point (`scripts/compatibility/run-opencypher-tck.ps1` or a cargo alias). **This number is the task's primary deliverable** — everything before it exists to make it trustworthy

## 4. Close the verified gaps
- [ ] 4.1 **Silent wrong-results bug:** `MATCH (n:$label)` parses (`parser/clauses/pattern.rs:506-522`, called for all node patterns at `:335`) but resolves nowhere on the read path, so it matches a *literal label named `$label`* instead of erroring (`engine/query_pipeline.rs:59-62`, sentinel handled only for CREATE at `:722-734`). Resolve the sentinel at execution start via catalog lookup. Note `WHERE n:$x` already works (`engine/tests/query.rs:888-893`) — mirror its semantics, including how it collapses NULL/empty/non-STRING to no rows
- [ ] 4.2 Regression test for 4.1 asserting the **wrong-results** behaviour specifically: a graph containing both a node labelled `Foo` and (if constructible) a node labelled literally `$label` must not be confused. A test that only checks "the right node is returned" would have passed before the fix in some graphs
- [ ] 4.3 Genuine parse gap: `parse_types` (`parser/clauses/pattern.rs:525-540`) has no `$` branch, so `-[r:$type]->` fails to parse. Add it, mirroring the label sentinel representation, then resolve it on the same path as 4.1
- [ ] 4.4 List-valued and invalid dynamic labels/types: fall back to AllNodesScan+Filter for LIST parameters; raise a typed error on non-STRING/LIST rather than silently matching nothing
- [ ] 4.5 `UNION` / `UNION ALL` inside `CALL { }`: this parses today (`parser/clauses/subquery.rs:88` → `parse_clause`, `clauses/mod.rs:284-287`); the gap is that `operators/call_subquery.rs` only inspects `Clause::Return` (`:134`, `:233`) and never `Clause::Union`. Wire the branches through the existing Union operator with per-branch scope validation (all branches must export identical columns)

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
