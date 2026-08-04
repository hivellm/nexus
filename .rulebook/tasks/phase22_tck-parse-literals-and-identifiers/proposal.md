# Proposal: phase22_tck-parse-literals-and-identifiers

> openCypher TCK re-baseline epic. Plan: docs/analysis/tck-rebaseline/
> Phase D — F-152 (RC19) (05-write-clauses.md)

## Why
The parser residue after the path-assignment, pattern-predicate, and projection-list
tasks are removed:

```
WITH {name:'Mats'} AS map RETURN map.`name` AS result -> Expected identifier   WRONG
... list slice / index forms                          -> Expected ']'   (21 rows)
... parenthesised precedence forms                    -> Expected ')'   (21 rows)
```

Backtick-delimited identifiers after `.` are the clearest single item. The `]` and `)`
families are mostly list-slice and precedence forms whose meaning depends on the postfix
work in phase22_tck-projection-list-silent-truncation — **re-measure after that lands;
part of this 78 will have disappeared.**

## What Changes
- Backtick-delimited identifiers in property access and aliases.
- The list index/slice grammar the corpus exercises (open-ended, negative, nested).
- The parenthesised precedence forms still rejected after the postfix work.

## TCK Impact
78 fails as measured today (precedence 18, list 16, match 7, create 6, with 6) — expected lower after the truncation task.

## Impact
- Affected code: crates/nexus-core/src/executor/parser/expressions/ (identifier.rs, precedence.rs, structured.rs)
- Breaking change: NO
- Dependencies: **Blocked by** phase22_tck-projection-list-silent-truncation — and re-measure before sizing.
- User benefit: the remaining valid Cypher spellings parse
