# Proposal: phase21_tck-temporal-projection-and-store

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 5 — temporal (03-temporal.md)

## Why
Returning modified temporal components and round-trip invariants (store/reload) for typed temporals.

## What Changes
- Temporal projection (select/override component subset).
- NexusValue encoding for store round-trip.
- Temporal arrays + null handling.

## TCK Impact
Temporal store/projection scenarios.

## Impact
- Affected code: crates/nexus-core/src/executor storage/property encoding + projection
- Breaking change: NO
- Dependencies: phase21_tck-temporal-typed-value.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
