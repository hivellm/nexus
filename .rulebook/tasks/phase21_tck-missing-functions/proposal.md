# Proposal: phase21_tck-missing-functions

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 1 — high-momentum small additions (04-expressions.md)

## Why
A cluster of small, low-risk pure additions. reverse() only handles arrays; startNode/endNode/properties missing; sign/rand/cot/haversin missing; range() with step=0 should error.

## What Changes
- fn_list.rs: reverse(string).
- fn_graph.rs: startNode/endNode/properties.
- fn_math.rs: sign, rand, cot, haversin.
- range() step != 0 validation.

## TCK Impact
string, graph, math scenarios directly.

## Impact
- Affected code: crates/nexus-core/src/executor/eval/projection/{fn_list,fn_graph,fn_math}.rs
- Breaking change: NO
- Dependencies: None.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
