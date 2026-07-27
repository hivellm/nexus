# Proposal: phase21_tck-temporal-accessors-and-properties

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 5 — temporal (03-temporal.md)

## Why
TCK uses property access (d.year, d.epochMillis, d.weekYear, d.ordinalDay, d.offsetMinutes, duration.seconds vs secondsOfMinute). Top-level functions exist but PropertyAccess only descends into Object -> Null on a temporal.

## What Changes
- PropertyAccess dispatch on typed temporals.
- ~50 derived accessor components (year/month/day/hour/.../weekYear/ordinalDay/offsetMinutes/...).

## TCK Impact
Temporal accessor scenarios (~50).

## Impact
- Affected code: crates/nexus-core/src/executor/eval/projection/core.rs, fn_temporal.rs
- Breaking change: NO
- Dependencies: phase21_tck-temporal-typed-value.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
