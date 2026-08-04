# Proposal: phase21_tck-path-null-and-semantics

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 6 — paths (05-quantifier-patterns-paths.md)

## Why
expressions/path (0%) fails on nodes(null)/relationships(null) (should be null) and length() type errors.

## What Changes
- nodes()/relationships() on null -> null.
- length() type checking (non-path -> error).

## TCK Impact
expressions/path null-handling scenarios.

## Impact
- Affected code: crates/nexus-core/src/executor/eval/.../fn_graph.rs
- Breaking change: NO
- Dependencies: phase21_tck-column-name-fidelity.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
