# Proposal: phase21_tck-temporal-comparison-and-arithmetic

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 5 — temporal (03-temporal.md)

## Why
Arithmetic covers date/datetime +/- duration and duration +/- duration but misses duration x/div number, fractional components, and +/-duration for time/localtime/localdatetime. Comparison is impossible without canonical rendering.

## What Changes
- duration x/div number.
- fractional duration components (P0.75M).
- +/-duration for time/localtime/localdatetime.
- Comparison/ordering within a kind.

## TCK Impact
Temporal arithmetic and comparison (~100+).

## Impact
- Affected code: crates/nexus-core/src/executor/eval/temporal.rs
- Breaking change: NO
- Dependencies: phase21_tck-temporal-typed-value, phase21_tck-temporal-iso8601-parsing.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
