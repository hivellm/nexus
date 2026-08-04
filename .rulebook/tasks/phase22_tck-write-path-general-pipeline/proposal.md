# Proposal: phase22_tck-write-path-general-pipeline

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase D — F-150 (RC10) (05-write-clauses.md)

## Why
**The plan's only genuinely architectural item.** Write queries do not run through the
general read pipeline; they take a narrower path that reimplements a subset of it and
refuses the rest at runtime, with seven distinct refusal messages:

| Query | Error |
|---|---|
| `MATCH (n:A) WHERE n.name = 'x' SET n.name = 'y' RETURN n` | `WHERE in a write query only supports id(var) = <value>` |
| `OPTIONAL MATCH (a:X) SET a.num = 42 RETURN a` | `OPTIONAL MATCH not supported in write queries` |
| `WITH 42 AS var MERGE (c:N {var: var})` | `Unsupported clause in write query` |
| `MATCH ()-[r]->() REMOVE r.num RETURN ...` | `Unknown variable 'r' in REMOVE clause` |
| `MATCH (n) REMOVE n.num RETURN n.num IS NOT NULL AS x` | `Unexpected character in expression` (the write parser lacks `IS NOT NULL`) |
| `CREATE (:X) CREATE (:X) MERGE (:X)` | `MERGE requires a variable alias` |
| `MERGE p = (a {num: 1}) RETURN p` | `Expected '(' at column 8` |

Widening the restricted path one clause at a time is how it reached seven refusals. The
right shape is to make the write clauses **operators inside the general pipeline**, so
`WHERE`, `WITH`, `OPTIONAL MATCH`, expression parsing, and variable binding are
inherited rather than reimplemented.

**Expect this task to need its own decomposition** — five steps, upstream first, each
independently shippable and each gated on the differential suite.

## What Changes
Decompose upstream-first:
1. full expression and `WHERE` parsing inside a write query;
2. relationship variables visible to `SET` / `REMOVE` / `DELETE`;
3. `WITH` before a write clause;
4. `OPTIONAL MATCH` plus null-row write semantics (a write over a null binding is a
   no-op that still yields its row);
5. `MERGE` without an alias, and `MERGE p = pattern`.

## TCK Impact
67 fails (set 26, remove 24, merge 13, create 2) plus the `Ignore null when ...` rows attributed to RC22.

## Impact
- Affected code: crates/nexus-core/src/executor/planner/queries/planner_core/, crates/nexus-core/src/executor/operators/ (write operators), crates/nexus-core/src/executor/parser/clauses/
- Breaking change: NO
- Dependencies: Benefits from phase22_tck-named-path-value (`MERGE p = ...`) and phase22_tck-projection-list-silent-truncation (inherited expression parsing).
- User benefit: write queries stop rejecting ordinary Cypher that read queries already accept
