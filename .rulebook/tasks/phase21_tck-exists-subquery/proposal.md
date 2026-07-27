# Proposal: phase21_tck-exists-subquery

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 6 — patterns (05-quantifier-patterns-paths.md)

## Why
Extend single-pattern EXISTS to the full `exists { subquery }` grammar with variable correlation. Clears 9 of 10 existentialSubqueries.

## What Changes
- Parse the EXISTS subquery form.
- Subquery correlation + variable scoping.
- Evaluate the EXISTS subquery.

## TCK Impact
expressions/existentialSubqueries (~9 of 10).

## Impact
- Affected code: crates/nexus-core/src/executor parser + executor + eval
- Breaking change: NO
- Dependencies: phase21_tck-semantic-analysis-pass.
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
