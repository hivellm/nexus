# Proposal: phase21_tck-parser-precedence-operators

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 3 — parser breadth W-E (04-expressions.md)

## Why
`^` is left-assoc and binds wrong (2^3^2, -2^2); `%` uses rem_euclid (-3 % 2 should be -1); comparison chaining stops after one operator.

## What Changes
- ^ right-assoc, tighter binding than * and unary.
- % Cypher truncated remainder (not rem_euclid).
- Comparison chaining a < b < c.

## TCK Impact
expressions/precedence (86), expressions/math.

## Impact
- Affected code: crates/nexus-core/src/executor/parser/expressions/precedence.rs, eval/arithmetic.rs
- Breaking change: NO
- Dependencies: None.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
