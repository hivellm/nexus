# Proposal: phase21_tck-semantic-analysis-pass

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Phase 4 — largest read-side lever, W-A/A2 (02-cross-cutting-workstreams.md)

## Why
The pipeline goes parser->planner->executor with ZERO validation stage. Negative tests that cannot be caught at eval (query 'succeeds' with wrong rows) require an AST scoping pass. This is the largest read-side lever (~241 negative-test scenarios) and the dominant lever the analysis identifies.

## What Changes
- New module executor/semantic_validation.rs (AST pass after parse).
- Variable scoping: VariableTypeConflict, VariableAlreadyBound, UndefinedVariable, NoVariablesInScope.
- Aggregation placement: AmbiguousAggregationExpression, NestedAggregation, agg-in-WHERE/ORDER BY.
- SKIP/LIMIT args: NegativeIntegerArgument, float type error, non-constant.
- Projection/UNION structure: ColumnNameConflict, DifferentColumnsInUnion, InvalidClauseComposition; write-side InvalidDelete, MERGE SemanticError.
- Emit OpenCypherErrorKind + CamelCase detail tokens; fix SemanticError dead-variant (error.rs:305).

## TCK Impact
match, match-where, return-skip-limit, union, with-where, set, create, merge, delete — ~241 + tail (largest read-side cluster).

## Impact
- Affected code: crates/nexus-core/src/executor/semantic_validation.rs (new), error.rs
- Breaking change: NO
- Dependencies: Phase 0 (baseline trust).
- User benefit: closes the conformance gap toward the TCK 100% target (baseline 13.2%).
