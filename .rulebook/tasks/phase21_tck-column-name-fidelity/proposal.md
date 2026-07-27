# Proposal: phase21_tck-column-name-fidelity

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 1 — global result-shape lever W-C (04-expressions.md / 07-execution-plan.md)

## Why
Columns render as bare `count` instead of the TCK's verbatim `count(*)` (strategy.rs, expressions.rs). Column-name mismatch masks otherwise-correct rows and is the first thing compare_table checks. Prerequisite for counting/path/quantifier scenarios to pass at all.

## What Changes
- expression_to_string must render the verbatim source text, not re-synthesise it.
- Aggregates (count/sum/avg/min/max/collect) canonicalised from source text.
- Normalise `<>`->`!=`, `null`->`NULL`, `1.0`->`1`, whitespace per TCK.

## TCK Impact
return, return-orderby, aggregation, literals (unnamed) — hundreds of scenarios.

## Impact
- Affected code: crates/nexus-core/src/executor/planner/queries/expressions.rs, strategy.rs
- Breaking change: NO
- Dependencies: None.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
