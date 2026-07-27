# Proposal: phase21_tck-non-finite-floats

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 7 — long tail, deprioritise (04-expressions.md)

## Why
serde_json::Number cannot hold NaN/+/-Infinity, so this is an architectural value-representation change (NaN-boxing or a custom Number type) for a low scenario count. Deferred to last.

## What Changes
- Value-representation upgrade (NaN-box or custom Number).
- Arithmetic operators handle NaN/Infinity.

## TCK Impact
Math/literals handful.

## Impact
- Affected code: crates/nexus-core/src/executor value representation
- Breaking change: NO
- Dependencies: Everything (last).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
