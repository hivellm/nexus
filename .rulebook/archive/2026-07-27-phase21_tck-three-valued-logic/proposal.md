# Proposal: phase21_tck-three-valued-logic

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 2 — highest scenario-per-effort, W-B + W-A1 (04-expressions.md)

## Why
value_to_bool maps Null->false and coerces; value_to_string maps Null->"null". Implementing strict 3VL in the two evaluators is the single highest scenario-per-effort change: it flips boolean from 4% to near-100% and cascades through precedence/comparison/null/list/string/map/typeConversion.

## What Changes
- Shared 3VL logical helper (Bool|Null else InvalidArgumentType).
- AND/OR/NOT strict 3VL with operand type-guards (123 AND true -> error).
- XOR: AST variant, lexing, precedence, eval arm.
- Comparison <,<=,>,>= : Null -> Null (not a sort default).
- IN, STARTS/ENDS/CONTAINS, list-slice null bounds: 3VL.
- toX invalid-type errors (toBoolean(1.0) -> error).
- Quantifier predicate null-propagation via 3VL.

## TCK Impact
expressions/boolean 4% -> near-100%; precedence, comparison, null, list, string, map, typeConversion — ~200-300 scenarios (largest single slice).

## Impact
- Affected code: crates/nexus-core/src/executor/eval/projection/core.rs, eval/predicate.rs
- Breaking change: NO
- Dependencies: Phase 0 (baseline trust).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
