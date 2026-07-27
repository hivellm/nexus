# Proposal: phase21_tck-binary-tree-fixtures

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 7 — harness + engine, opportunistic (01-measurement-and-methodology.md)

## Why
19 skip-listed scenarios depend on a canonical binary-tree-N fixture behind a Given step; passing also depends on triadic MATCH support (useCases/triadicSelection).

## What Changes
- Harness: `Given the binary-tree-N graph` -> hard-coded CREATE scripts.
- Triadic selection MATCH support as needed.

## TCK Impact
~19 skip-list scenarios.

## Impact
- Affected code: tests/tck_opencypher.rs + executor
- Breaking change: NO
- Dependencies: Phase 5 (if fixtures use complex-literal CREATE setups).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
