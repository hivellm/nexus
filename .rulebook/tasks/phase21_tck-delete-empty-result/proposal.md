# Proposal: phase21_tck-delete-empty-result

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 1 — quick win, highest ROI (06-clauses-read-write.md)

## Why
`DELETE (n)` with no RETURN returns 1 row `{count:N}` (query_pipeline.rs:694-702); the TCK expects zero rows. Uniform failure. Highest-ROI single fix in the write clauses.

## What Changes
- RETURN-less mutations must produce an empty result set.
- Apply only when the statement has no RETURN clause.

## TCK Impact
clauses/delete 0% -> ~90%, plus tail scenarios.

## Impact
- Affected code: crates/nexus-core/src/engine/query_pipeline.rs, result_set_ops
- Breaking change: NO
- Dependencies: None.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
