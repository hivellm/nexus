# Proposal: phase21_tck-temporal-durationbetween

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 5 — temporal (03-temporal.md)

## Why
Duration must render as a canonical ISO string (currently a JSON object). Need sub-second precision, sign rules (PT-1.999S), unit normalisation (seconds:70 -> ...M...S), tz/offset formatting.

## What Changes
- durationBetween(a,b) family.
- Canonical duration ISO-string rendering.
- Sub-second precision, sign rules, unit normalisation.

## TCK Impact
Temporal durationBetween (~164 lines, Temporal4.feature).

## Impact
- Affected code: crates/nexus-core/src/executor/eval/.../fn_temporal.rs
- Breaking change: NO
- Dependencies: phase21_tck-temporal-typed-value, phase21_tck-temporal-iso8601-parsing.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
