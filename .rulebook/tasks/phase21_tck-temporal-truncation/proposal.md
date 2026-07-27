# Proposal: phase21_tck-temporal-truncation

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 5 — temporal, largest sub-block (03-temporal.md)

## Why
Temporal9.feature is the largest single sub-block (~337 lines, 5 types x ~10 units). No truncate() exists anywhere.

## What Changes
- Implement truncate(value, unit) for YEAR/MONTH/DAY/HOUR/MINUTE/SECOND/MILLISECOND.
- Handle all temporal kinds.

## TCK Impact
Temporal truncation (~337 lines).

## Impact
- Affected code: crates/nexus-core/src/executor/eval/.../fn_temporal.rs
- Breaking change: NO
- Dependencies: phase21_tck-temporal-typed-value, phase21_tck-temporal-iso8601-parsing.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
