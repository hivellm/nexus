# Proposal: phase21_tck-pattern-comprehension-output

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 6 — patterns (05-quantifier-patterns-paths.md)

## Why
Pattern comprehension [p = (n)-->() | e] evaluates but its output shape is not corrected to the TCK path-value shape.

## What Changes
- Emit proper @tck_path-shaped path values from pattern comprehensions.

## TCK Impact
expressions/pattern comprehension scenarios.

## Impact
- Affected code: crates/nexus-core/src/executor/eval/projection/core.rs
- Breaking change: NO
- Dependencies: phase21_tck-column-name-fidelity.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
