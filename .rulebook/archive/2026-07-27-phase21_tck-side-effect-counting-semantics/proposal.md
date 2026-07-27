# Proposal: phase21_tck-side-effect-counting-semantics

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 1 — result-shape lever W-C (06-clauses-read-write.md)

## Why
Counters exist (types.rs:161-189) but diverge from spec: property overwrite counts only +1 (should be +1/-1); +labels counts per (node,label) (should be per-statement); null-valued map keys count (should not). Exact assert_eq kills these across create/merge/set/remove/delete.

## What Changes
- +labels: count once per statement, not count_ones() per node.
- +properties/-properties: overwrite counts both (+1 and -1).
- Null-valued map keys: skip in property counts.

## TCK Impact
set, create, merge, remove, delete — ~150-250 scenarios (overlap with semantic pass).

## Impact
- Affected code: crates/nexus-core/src/engine/record_store_ops.rs, write_exec.rs
- Breaking change: NO
- Dependencies: None (land before/with per-clause feature work).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
