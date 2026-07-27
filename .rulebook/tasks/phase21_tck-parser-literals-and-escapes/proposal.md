# Proposal: phase21_tck-parser-literals-and-escapes

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 3 — parser breadth W-E (04-expressions.md)

## Why
parse_numeric_literal handles only decimal + `.`; parse_string_literal handles only \n\t\r\\. Unlocks most of the literals category and part of string.

## What Changes
- Numeric: 1e10, 0x, 0o, 1_000, .5 forms.
- String: \uXXXX, \b, \f, \0 escapes.

## TCK Impact
expressions/literals (76), expressions/string.

## Impact
- Affected code: crates/nexus-core/src/executor/parser/expressions/primary.rs
- Breaking change: NO
- Dependencies: None.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
