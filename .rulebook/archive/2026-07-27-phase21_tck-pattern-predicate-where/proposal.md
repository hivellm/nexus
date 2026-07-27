# Proposal: phase21_tck-pattern-predicate-where

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 6 — patterns (05-quantifier-patterns-paths.md)

## Why
EXISTS/PatternComprehension AST + parsing exist and eval via check_pattern_exists exists, but helpers.rs:840/851 short-circuits to false without graph context.

## What Changes
- Wire graph context into EXISTS-in-WHERE evaluation.
- Support anonymous rel/node, undirected, var-length (n)-[:T*]-().
- Negation/conjunction.

## TCK Impact
expressions/pattern pattern-predicate scenarios.

## Impact
- Affected code: crates/nexus-core/src/executor/eval/.../helpers.rs
- Breaking change: NO
- Dependencies: phase21_tck-column-name-fidelity, phase21_tck-three-valued-logic.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
