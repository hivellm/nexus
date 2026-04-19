# Proposal: phase3_unwind-where-neo4j-parity

## Why

Compat test **14.05** (`UNWIND [1,2,3,4,5] AS x WHERE x > 2 RETURN x`) is
the single outlier in the 300-test Neo4j compatibility suite — it's the
only test excluded from the 299/300 pass baseline that `phase3_executor-
columnar-wiring` re-confirmed on 2026-04-19. The failure mode isn't
Nexus being wrong about the result — it's Nexus being **too permissive
about the grammar**.

Side-by-side repro against live Nexus 0.12.0 + Neo4j 2025.09.0 community:

```
=== NEXUS ===
{"columns":["x"],"rows":[[3],[4],[5]],"execution_time_ms":0}

=== NEO4J ===
{"errors":[{"code":"Neo.ClientError.Statement.SyntaxError",
  "message":"Invalid input 'WHERE': expected 'ORDER BY', 'CALL',
  'CREATE', 'LOAD CSV', 'DELETE', 'DETACH', 'FINISH', 'FOREACH',
  'INSERT', 'LIMIT', 'MATCH', 'MERGE', 'NODETACH', 'OFFSET',
  'OPTIONAL', 'REMOVE', 'RETURN', 'SET', 'SKIP', 'UNION', 'UNWIND',
  'USE', 'WITH' or <EOF> (line 1, column 29 (offset: 28))"}]}
```

Standard Cypher only allows `WHERE` as part of `MATCH` / `OPTIONAL
MATCH` / `WITH` — never as a standalone top-level clause. The valid
form of test 14.05 is:

```cypher
UNWIND [1, 2, 3, 4, 5] AS x WITH x WHERE x > 2 RETURN x
```

Nexus's parser at [clauses.rs:181](../../nexus-core/src/executor/parser/clauses.rs#L181)
accepts `"WHERE"` as a standalone arm in the top-level clause-dispatch
match, which lets it appear in any position between other clauses.
That's what causes the divergence.

Closing this gap is the last step to **300/300** on the Neo4j
compatibility suite.

## What Changes

- **Parser tightening.** Remove the standalone `"WHERE"` arm in
  `nexus-core/src/executor/parser/clauses.rs::parse_clause` (line
  181). Emit a Neo4j-style syntax error listing the clauses that
  WHERE cannot directly follow — the same text shape Neo4j
  produces, so the error message is useful in migration.
- **`MATCH` / `OPTIONAL MATCH` / `WITH` paths stay unchanged.**
  Those already call `parse_where_clause` internally
  (line 1172 already handles it for MATCH; WITH has a parallel
  attachment); their WHERE handling is correct and spec-compliant.
  Only the top-level standalone dispatch goes away.
- **Internal query migration.** 4 tests use the shorthand today:
  `nexus-core/tests/unwind_tests.rs` (3 occurrences at lines 126,
  159, 348) and `nexus-core/tests/executor_comprehensive_test.rs`
  (1 occurrence). Each rewrites from `UNWIND … AS x WHERE …` to
  `UNWIND … AS x WITH x WHERE …`. The rewrite is mechanical and
  semantics-preserving — `WITH x` is a pass-through projection.
- **Compat test 14.05 rewrite.** Update
  `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`
  line 737 to the valid form so both sides return matching data
  instead of diverging on a syntax error. Alternative — leave it
  and assert both sides return syntax errors — is less useful
  because the diff harness does text comparison and the two error
  messages don't match verbatim.
- **Changelog + migration note.** Document the tightening in
  `CHANGELOG.md` under "Fixed" (or "Changed" if we agree it's a
  breaking change). Point users at the `WITH x` rewrite pattern
  for any queries affected.

## Impact

- **Affected specs**: `docs/specs/cypher-subset.md` (mention standalone
  WHERE is now rejected, matching Neo4j), implicit impact on any spec
  that describes clause ordering.
- **Affected code**:
  - `nexus-core/src/executor/parser/clauses.rs` (remove standalone
    WHERE arm, add rejection path + test).
  - `nexus-core/tests/unwind_tests.rs` (rewrite 3 queries).
  - `nexus-core/tests/executor_comprehensive_test.rs` (rewrite 1
    query).
  - `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`
    (rewrite test 14.05 query).
  - `CHANGELOG.md` (new entry).
- **Breaking change**: YES — any user query using `UNWIND … AS x
  WHERE …` (or `CREATE … WHERE …`, `DELETE … WHERE …`, etc. — the
  leniency covers more than UNWIND) breaks. The rewrite is
  mechanical (`WITH <vars> WHERE …`), but callers need to know.
  Mitigation: the syntax error that replaces the silent acceptance
  tells them exactly what to do.
- **User benefit**: 300/300 Neo4j compatibility on the reference
  suite — the cleanest claim we can make, and the one the README
  already points at. Also removes a class of queries Nexus accepts
  that will silently fail when ported to Neo4j.

## Out of Scope

- Other over-permissive parser leniencies (e.g. RETURN ordering,
  clause repetition rules) — each should be its own phase-3 task
  with its own compat evidence.
- `WITH` / intermediate-projection semantics — already correct.
- Any performance work on the WHERE predicate path — orthogonal.

## Source

- Compat suite exclusion: `scripts/compatibility/test-neo4j-nexus-
  compatibility-200.ps1` line 737 — the single "14.05 UNWIND with
  WHERE" entry.
- Parser site: `nexus-core/src/executor/parser/clauses.rs` line 181
  ("WHERE" arm in `parse_clause` top-level dispatch).
- Internal test sites: `nexus-core/tests/unwind_tests.rs` lines
  126, 159, 348; `nexus-core/tests/executor_comprehensive_test.rs`
  (one occurrence).
- 2026-04-19 reference run: 299/300 pass, 14.05 the only
  exclusion, archived with `phase3_executor-columnar-wiring`.
