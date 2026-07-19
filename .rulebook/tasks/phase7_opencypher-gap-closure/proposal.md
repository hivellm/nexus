# Proposal: phase7_opencypher-gap-closure

> **Revised 2026-07-19 after verifying every claim in the original draft.** The core
> premise held; four of the supporting claims did not. Corrections are marked
> **[REVISED]** inline so the original reasoning stays auditable.

## Why

Nexus has **no measurable openCypher conformance number**, and its own documentation
disagrees with itself by 40 points. That is the actual problem this task exists to fix.

Verified (evidence in each bullet):

- **No upstream TCK is vendored.** Exactly 4 `.feature` files exist repo-wide, all
  under `crates/nexus-core/tests/tck/spatial/`. No `vendor/`, no submodules, no
  build-time fetch. CONFIRMED.
- **The advertised "300/300" is a live Neo4j differential suite, not the TCK.**
  `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` POSTs each case to
  both `$Neo4jUri/db/neo4j/tx/commit` (`:44`) and Nexus, then diffs
  (`Compare-QueryResults`, `:95`). It requires a live Neo4j on `:7474`. CONFIRMED.
  Its real count is **325** `Run-Test` invocations — the file header still says "300
  Tests" (`:1`, `:21`) and is stale. **[REVISED]** Also: the `.sh` sibling has drifted
  to only **196** cases and is effectively abandoned — reconcile or delete it.
- **The spatial corpus is Nexus-authored, 22 scenarios**, because upstream ships no
  spatial scenarios at all (documented with a reproduction command at
  `tests/tck/spatial/VENDOR.md:30-70`). CONFIRMED.
- **The docs contradict each other**: `AGENTS.override.md:159`, `docs/PRD.md:24`,
  `docs/ROADMAP.md:6`, `docs/guides/USER_GUIDE.md:26` all say ~55%;
  `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md:84` says "toward ~95%";
  `docs/nexus/README.md:22` says "~85% (better than the docs claim)". None is measured.

**[REVISED] The original draft's "real coverage ~65–70%" is dropped.** No repo
artifact supports it; it was an estimate presented as a finding. This task's job is to
*produce* a defensible number, not to assert one. Adding a seventh unmeasured figure to
a set that already spans 55/85/95 would make the problem worse.

The upstream corpus is small enough to vendor outright: **220 feature files, 1615
scenarios, 2.1 MB** at `tck/features/` (measured against
`opencypher/openCypher@677cbafabb8c3c5eed458fd3b1ec0daec8d67d23`). Network access to
the upstream repo is available.

## What Changes

### [REVISED] Corrections to the original gap list

**Gap "CREATE CONSTRAINT … FOR … REQUIRE is unsupported" — REFUTED, dropped.**
It is fully implemented at
`crates/nexus-core/src/executor/parser/clauses/admin.rs:311-324` with the FOR/REQUIRE
body parser at `:413-437`, covering `IS UNIQUE`, `IS NOT NULL`, `IS NODE KEY`,
`IS :: <TYPE>`, the relationship form `FOR ()-[r:T]-() REQUIRE …`, optional constraint
names and `IF NOT EXISTS`. Parser tests exist (`parser/tests/ddl.rs:186,205,222,239,256`).
The draft cited `docs/specs/cypher-subset.md:~1562` as marking it Unsupported; that
file contains **zero occurrences of "REQUIRE"**. All that remains is a documentation
bug: a shipped feature is undocumented.

**Gap "read-side dynamic labels are rejected by the parser" — misdiagnosed, and the
truth is worse.** `parse_labels` (`executor/parser/clauses/pattern.rs:506-522`) already
accepts `:$param` and encodes a `"$param"` sentinel, and it is called for *all* node
patterns (`pattern.rs:335`). So `MATCH (n:$label)` parses cleanly — then silently
matches a *literal label named `$label`*, because sentinel resolution only runs on the
CREATE path (`engine/query_pipeline.rs:722-734`; see the note at `:59-62`). **This is a
silent wrong-results bug, not a missing feature** — it returns confidently incorrect
data rather than an error, which is the more dangerous failure mode and changes both
the fix and the regression test.
Two adjacent facts the draft missed: `WHERE n:$x` **already works**
(`engine/tests/query.rs:888-893`), and relationship types are a genuine *parse-level*
gap — `parse_types` (`pattern.rs:525-540`) has no `$` branch, so `-[r:$type]->` fails
to parse. Only that half was correctly characterised.

**Gap "UNION inside CALL { } is rejected by the parser" — misdiagnosed.**
`parse_call_subquery_clause` (`executor/parser/clauses/subquery.rs:88`) delegates to
`parse_clause`, which handles `UNION` (`clauses/mod.rs:284-287`), so it parses. The gap
is executor wiring: `executor/operators/call_subquery.rs` only ever inspects
`Clause::Return` (`:134`, `:233`) and never `Clause::Union`. The draft cited
`ast.rs`; the relevant code is in `clauses/subquery.rs` and `operators/call_subquery.rs`.
The gap is real, but it is a planner/executor task, not a grammar task.

**[REVISED] The four "tracked elsewhere" dependencies are all shipped.**
`phase6_opencypher-quantified-path-patterns`, `phase6_opencypher-subquery-transactions`,
`phase6_opencypher-geospatial-predicates` and `phase7_planner-using-index-hints` are all
in `.rulebook/archive/` with every checkbox ticked. They are not blockers. (Minor
hygiene: the subquery-transactions `.metadata.json` still reads `"status": "pending"`
despite being archived.)

### [REVISED] Harness prerequisites the draft under-scoped

The existing runner (`crates/nexus-core/tests/tck_runner.rs`) drives 22 hand-authored
scenarios. Two of its steps are not merely incomplete but **silently vacuous**, and
both are load-bearing across the real TCK:

- `no side effects` (`:190-197`) is a **no-op stub** — `ResultSet` exposes no
  side-effect counters. A large share of TCK scenarios assert on created/deleted
  node/relationship/property counts, and every one of them would pass vacuously.
- `a <X> should be raised at runtime: <token>` (`:170-188`) **ignores the error kind**
  and does a substring match, because Nexus has no openCypher error taxonomy (`:162-169`).

The hand-rolled table-cell parser (`:333-549`) also covers only what the spatial corpus
needs — no temporal/duration/node/relationship/path literals, all of which upstream
tables use heavily.

**A pass rate produced before these are fixed would be inflated and untrustworthy** —
which is precisely the disease this task is treating. They are therefore prerequisites
of the baseline, not follow-ups.

### Deliverables

- Vendor the TCK under `crates/nexus-core/tests/tck/opencypher/` with the upstream
  commit pinned and licence attribution, keeping the Nexus-authored spatial corpus
  separate.
- Generalise the Gherkin harness: real side-effect assertions, a real error taxonomy,
  a table-cell parser covering the upstream literal set, and an explicit skip-list for
  categories that are knowingly unsupported (skips must be *counted and reported*, never
  silently dropped).
- Publish a per-category baseline as `docs/compatibility/OPENCYPHER_TCK_REPORT.md`, plus
  a reproducible runner entry point.
- Fix the read-side dynamic-label wrong-results bug; add `-[r:$type]->` parsing.
- Wire `UNION` / `UNION ALL` through `CALL { }` in the executor.
- **[REVISED]** Reconcile the compatibility claim across `AGENTS.override.md:159`,
  `docs/PRD.md:24`, `docs/ROADMAP.md:6`, `docs/guides/USER_GUIDE.md:26`,
  `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md:84` and `docs/nexus/README.md:22`
  with the measured number. **Do not edit `CLAUDE.md`** as the draft proposed — it is
  generated between `RULEBOOK:START/END` sentinels and says DO NOT EDIT BY HAND at
  `:1-3`; it does not mention openCypher at all.
- Document the shipped-but-undocumented `FOR … REQUIRE` constraint syntax.

## Impact

- Affected specs: `docs/specs/cypher-subset.md`,
  `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`, new
  `docs/compatibility/OPENCYPHER_TCK_REPORT.md`
- Affected code: `crates/nexus-core/tests/tck/**`, `crates/nexus-core/tests/tck_runner.rs`,
  `crates/nexus-core/src/executor/parser/clauses/pattern.rs`,
  `crates/nexus-core/src/engine/query_pipeline.rs`,
  `crates/nexus-core/src/executor/operators/call_subquery.rs`, plus the six doc files above
- Breaking change: NO for query behaviour (additive grammar + a test harness). One
  behavioural correction is user-visible: `MATCH (n:$label)` stops matching a literal
  label named `$label`. Anyone depending on that was getting wrong results.
- User benefit: an honest, reproducible conformance number that replaces six mutually
  contradictory guesses; and dynamic labels stop silently returning wrong data.

## References

- Upstream: `opencypher/openCypher@677cbafabb8c3c5eed458fd3b1ec0daec8d67d23`,
  `tck/features/` — 220 files, 1615 scenarios, 2.1 MB (measured 2026-07-19)
- Existing harness and its limits: `crates/nexus-core/tests/tck_runner.rs:170-197`, `:333-549`
- Why no spatial scenarios upstream: `crates/nexus-core/tests/tck/spatial/VENDOR.md:30-70`
- Diff suite: `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` (325 cases)
