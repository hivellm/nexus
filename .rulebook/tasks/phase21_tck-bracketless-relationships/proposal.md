# Proposal: phase21_tck-bracketless-relationships

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 1 — parser breadth W-E (06-clauses-read-write.md)

## Why
parser/clauses/pattern.rs:381 unconditionally requires `[`, so `MATCH (a)-->(b)` errors. A single-branch parser change turns hard errors into direct passes for a high-value positive-query gap (~54 scenarios).

## What Changes
- parse_relationship_pattern: add an anonymous-relationship branch with no `[`.
- parse_types: support colon-prefixed alternation `:A|:B`.

## TCK Impact
match, match-where (~54 scenarios).

## Impact
- Affected code: crates/nexus-core/src/executor/parser/clauses/pattern.rs
- Breaking change: NO
- Dependencies: None.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
