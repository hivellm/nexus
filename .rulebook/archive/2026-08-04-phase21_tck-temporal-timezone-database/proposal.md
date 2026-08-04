# Proposal: phase21_tck-temporal-timezone-database

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 5 — temporal, deferrable/hardest (03-temporal.md)

## Why
datetime()/time() ignore the timezone map key. chrono-tz/jiff is only a transitive dep. The tz-free slice (date/local*/duration) is substantial and reachable first; schedule zoned work AFTER the tz-free slice is complete.

## What Changes
- Promote chrono-tz (or adopt jiff) to a direct dep.
- datetime()/time() respect the timezone map key.
- Named zones, DST, historical offsets, epochMillis.

## TCK Impact
Zoned datetime/time scenarios, epochMillis, DST.

## Impact
- Affected code: Cargo.toml, crates/nexus-core/src/executor/eval/.../fn_temporal.rs
- Breaking change: NO
- Dependencies: phase21_tck-temporal-typed-value .. -truncation (tz-free slice first).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
