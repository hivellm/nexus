# Proposal: phase21_tck-parser-label-whitespace

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Surfaced repeatedly during the temporal chain (Temporal8[6] setup blocker).

## Why
`MATCH (dur2: Duration2)` — whitespace between the label colon and the label
name — fails with "Cypher syntax error: Expected identifier at line 1, column
30". openCypher permits whitespace there (the TCK uses it in fixtures), so
every scenario whose Given block or query writes `(var: Label)` dies at parse
time regardless of the feature under test. Confirmed blocking all 9 example
rows of Temporal8 scenario [6] (duration arithmetic — the arithmetic itself is
implemented and verified via unit tests); the same idiom likely appears
elsewhere in the corpus.

## What Changes
Accept optional whitespace between `:` and the label identifier in node (and
relationship-type, if the same gap exists — audit) patterns in the Cypher
tokenizer/pattern parser (crates/nexus-core/src/executor/parser/ — tokens.rs /
clauses/pattern.rs). Grammar-level fix, no semantic change. Ensure `WHERE
n:Label` predicate syntax and map-literal `{key: value}` parsing are not
disturbed (the colon there is followed by whitespace legitimately — the fix is
scoped to the label position after a variable, not a global token change).

## Impact
- Affected specs: docs/specs/cypher-subset.md (pattern grammar)
- Affected code: crates/nexus-core/src/executor/parser/ (tokens.rs and/or
  clauses/pattern.rs)
- Breaking change: NO (strictly widens accepted syntax)
- User benefit: TCK fixtures using the spaced idiom parse; Temporal8[6]'s 9
  rows unblock immediately (arithmetic already implemented); closes a
  Neo4j-parity parsing gap users can hit with hand-written queries.
