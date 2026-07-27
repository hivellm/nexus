# Proposal: phase21_tck-undirected-self-loop-counting

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 6 — counting (05-quantifier-patterns-paths.md)

## Why
useCases/countingSubgraphMatches (0%) fails uniformly on undirected/self-loop match counting (self-loop once; directed edge under `--` twice).

## What Changes
- Self-loop counted once, not twice.
- Undirected `--` counted once per direction in a directed context.

## TCK Impact
useCases/countingSubgraphMatches.

## Impact
- Affected code: crates/nexus-core/src/executor match evaluation
- Breaking change: NO
- Dependencies: phase21_tck-column-name-fidelity.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
