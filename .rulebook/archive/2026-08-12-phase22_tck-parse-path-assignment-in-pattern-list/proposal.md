# Proposal: phase22_tck-parse-path-assignment-in-pattern-list

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase B — F-130 (RC07) (04-read-clauses.md)

## Why
`p = pattern` parses only as the **first** element of a MATCH pattern list; after a
comma the pattern-list parser demands `(` immediately.

```
MATCH r = ()-[]-() RETURN r          -> parses                                      OK
MATCH (), r = ()-[]-() RETURN r      -> Cypher syntax error: Expected '(' at col 12  WRONG
MATCH ()-[]-(), r = ()-[]-(), () ... -> same                                         WRONG
```

The prefix is recognised at `parser/clauses/read.rs:14-37` only at the head of the
clause.

**Sequencing matters:** 64 of the 66 attributed failures are negative tests whose
assertion is `expected error to contain VariableTypeConflict`. They currently fail with
a *parse* error instead, so they flip only once
`phase22_tck-validate-variable-reuse` also lands. This task unblocks that one; its own
64 count materialises there.

## What Changes
- Accept a `variable =` path-assignment prefix on **any** element of a comma-separated
  pattern list, in MATCH, OPTIONAL MATCH, and MERGE alike.

## TCK Impact
66 fails (match 64, merge 2) — realised jointly with phase22_tck-validate-variable-reuse.

## Impact
- Affected code: crates/nexus-core/src/executor/parser/clauses/read.rs, crates/nexus-core/src/executor/parser/clauses/pattern.rs
- Breaking change: NO
- Dependencies: None. **Blocks** phase22_tck-validate-variable-reuse.
- User benefit: named paths can appear anywhere in a multi-pattern MATCH
