# Proposal: phase21_tck-temporal-iso8601-parsing

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 5 — temporal (03-temporal.md)

## Why
date() only parses %Y-%m-%d; duration() only a map, never an ISO string. Temporal2.feature tests the full surface with unit carry (P5M1.5D -> P5M1DT12H).

## What Changes
- Upgrade date parsing: calendar/week/ordinal/compact/partial forms.
- duration() ISO-8601 string constructor + parser (chrono can't parse ISO durations; jiff/custom).
- Unit-carry normalisation (P5M1.5D).

## TCK Impact
Temporal string-construction (~37 direct) + round-trip.

## Impact
- Affected code: crates/nexus-core/src/executor/eval/.../fn_temporal.rs, Cargo.toml
- Breaking change: NO
- Dependencies: phase21_tck-temporal-typed-value.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
