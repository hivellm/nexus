# Soft tests that accept errors as "not yet implemented" hide total feature breakage

**Category**: testing
**Tags**: testing, explain, profile, parser, soft-test

## Description

tests/query_analysis_test.rs guarded every EXPLAIN/PROFILE assertion with `if result.is_err() { tracing::info!("not yet implemented"); return; }`. EXPLAIN/PROFILE were COMPLETELY broken at the top level (missing from is_clause_boundary, every such query parsed to an empty AST) and the suite stayed green for months. A test that passes on both success and failure is not a test — it is documentation of hope. If a feature may legitimately be unimplemented, assert the SPECIFIC error variant; never blanket-accept any Err.

## Example

// WRONG: passes whether the feature works or is totally broken
if result.is_err() { return; }
// RIGHT: pin the expectation
let rs = result.expect("EXPLAIN must parse and plan");
assert_eq!(rs.columns, vec!["plan"]);

## When to Use

Reviewing or writing tests for features flagged as partially implemented.
