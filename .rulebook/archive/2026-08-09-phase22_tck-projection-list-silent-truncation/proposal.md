# Proposal: phase22_tck-projection-list-silent-truncation

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase A — F-110 (RC14) (02-projection-and-expressions.md)

## Why
**The most dangerous defect in the corpus.** On reaching an expression form it cannot
continue, the projection-list parser stops, keeps what it has, and discards the rest of
the list — including the `AS` alias — without raising an error.

```
RETURN false = true IS NULL AS a, false = (true IS NULL) AS b, (false = true) IS NULL AS c
  -> 1 column named "false = true"                      (want 3: a, b, c)
MERGE (a)-[r:KNOWS]-(b) RETURN startNode(r).id AS s, endNode(r).id AS e
  -> 1 column named "startNode(r)"                      (want 2: s, e)
WITH [123, {existing: 42}] AS list RETURN (list[1]).missing, (list[1]).existing
  -> 1 column named "list[1]"                           (want 2)
```

A worse variant drops a whole clause:

```
UNWIND [true] AS a WITH collect(a) AS eq
RETURN all(x IN eq WHERE x) AND any(x IN eq WHERE x) AS result
  -> columns ["eq"]                                     (the RETURN was ignored)
```

The engine answers a different question and reports success. It also makes RC18
(column names) and part of RC19 (parser) unmeasurable, so this must land before either
is sized.

## What Changes
- The parser must **error** when a projection item cannot be fully consumed. Silent
  truncation of a projection list, and silent dropping of a clause, are both removed.
- Then add the missing postfix forms that caused the truncations: `IS NULL` /
  `IS NOT NULL` as a postfix operator inside a larger expression, `.prop` applied to a
  call result or a parenthesised expression, and comparison chaining.

## TCK Impact
~90 fails (carved out of the column-count and column-name buckets): precedence 55, return 15, map 9, merge 7, string 3.

## Impact
- Affected code: crates/nexus-core/src/executor/parser/expressions/ (precedence.rs, structured.rs), crates/nexus-core/src/executor/parser/clauses/
- Breaking change: NO
- Dependencies: None. **Blocks** phase22_tck-parse-literals-and-identifiers and phase22_tck-column-name-fidelity-residue.
- User benefit: no more silently wrong answers from a query the parser could not fully read
