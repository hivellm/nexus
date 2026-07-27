# Proposal: phase21_tck-quantifier-in-where-parser

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 1 — parser breadth W-E (05-quantifier-patterns-paths.md)

## Why
any/all/none/single fall through to normal arg parsing; `IN` parses before `WHERE`, leaving a dangling WHERE -> parse error. The `filter()` special IN..WHERE form already exists; mirror it. Unblocks ~all 604 quantifier scenarios to reach evaluation (3VL later flips the majority).

## What Changes
- Mirror the filter() IN..WHERE parse block for any/all/none/single.
- Evaluators (fn_list.rs:319-473) already expect the correct shape.

## TCK Impact
expressions/quantifier 2.6% -> reachable evaluation (3VL then flips the majority).

## Impact
- Affected code: crates/nexus-core/src/executor/parser/expressions/identifier.rs
- Breaking change: NO
- Dependencies: None (Phase 2 does 3VL).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
