# Proposal: phase21_tck-temporal-typed-value

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 5 — temporal subsystem gate, W-D (03-temporal.md)

## Why
Temporals are faked as String (date/time) or Object (duration), so a duration can NEVER equal the TCK's ISO String expectation. Nothing else in temporal is trustworthy without a real typed value. This is the structural gate for the entire temporal subsystem (~935 failing scenarios).

## What Changes
- Add typed temporal representation (NexusValue::Date/DateTime/... or tagged-JSON enum).
- Constructors return the typed value, not String/Object.
- PropertyAccess on temporals + derived component names.
- Canonicalisation pass at RETURN boundary -> ISO String; never leak the tagged form into /cypher responses.
- Zero parser work (Cypher has no temporal literals).

## TCK Impact
Temporal ~935 (gate unblock) + with-orderBy temporal-gated (~120).

## Impact
- Affected code: crates/nexus-core/src/executor/types.rs, eval/projection/core.rs, eval/.../fn_temporal.rs
- Breaking change: NO
- Dependencies: None (it is the gate).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
