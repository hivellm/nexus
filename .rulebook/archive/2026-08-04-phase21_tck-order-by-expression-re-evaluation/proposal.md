# Proposal: phase21_tck-order-by-expression-re-evaluation

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 6 — ordering (07-execution-plan.md)

## Why
execute_sort silently skips unresolved sort keys (project.rs:594-599) and never re-evaluates the expression. Cross-type ordering also diverges (number < string < ...).

## What Changes
- Re-evaluate ORDER BY expressions against bindings (not column-name only).
- Cross-type comparison order per openCypher.

## TCK Impact
return-orderby, genuine with-orderBy ordering fails (~10).

## Impact
- Affected code: crates/nexus-core/src/executor/operators/project.rs
- Breaking change: NO
- Dependencies: None (independent).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
