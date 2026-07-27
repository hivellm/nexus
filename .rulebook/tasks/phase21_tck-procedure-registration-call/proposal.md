# Proposal: phase21_tck-procedure-registration-call

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 7 — long tail, deprioritise (06-clauses-read-write.md)

## Why
50 skip-listed; the corpus has zero `CALL { subquery }` (all `CALL proc()`), so there is little realistic conformance gain without full procedure support. Engine work is large, product value low. Deprioritised.

## What Changes
- Ad-hoc procedure registration.
- CALL/YIELD execution.

## TCK Impact
clauses/call (50 skip -> maybe ~10 realistic).

## Impact
- Affected code: crates/nexus-core/src/executor procedures + CALL
- Breaking change: NO
- Dependencies: Everything.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
