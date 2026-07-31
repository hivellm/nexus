# Proposal: phase21_tck-create-function-call-properties

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Surfaced by the temporal-accessors TCK gate: all 7 Temporal5.feature
> scenarios fail at fixture setup, never reaching the code under test.

## Why
Standalone `CREATE (:Val {p: date({...})})` rejects any function-call property
value with "Cypher execution error: Complex expressions not supported in CREATE
properties" (`expression_to_json_value` has no FunctionCall arm). The row-aware
CREATE path (`resolve_property_expr_for_create`, used when a row context exists
— e.g. UNWIND ... CREATE) already evaluates function calls and canonicalizes
temporals before storage. TCK fixtures across categories use the standalone
idiom, so every temporal accessor/comparison/truncation scenario whose Given
block builds nodes with `date(...)`/`duration(...)` properties is blocked at
setup regardless of whether the feature under test works. This gates the
measured TCK gains of the entire temporal chain.

## What Changes
Route standalone-CREATE property values through the same evaluator the
row-aware path uses (function calls evaluated, result canonicalized via
`temporal_value::canonicalize_value_in_place` before storage), preserving the
existing rejection only for genuinely unsupported expression classes (e.g.
references to variables that do not exist in scope). Audit MERGE and CREATE-in
-FOREACH property paths for the same limitation while there.

## Impact
- Affected specs: docs/specs/cypher-subset.md (CREATE property expressions)
- Affected code: crates/nexus-core/src/executor/operators/create.rs
  (expression_to_json_value / standalone path), possibly
  engine/match_exec.rs + engine/write_exec/dispatch.rs equivalents
- Breaking change: NO (turns an error into the documented behavior)
- User benefit: CREATE with computed property values works everywhere;
  necessary unlock for Temporal5's 7 scenarios (accessor implementation is
  already in place) and every other TCK fixture using the same idiom.
- Dependency note (from the accessors review): this task alone does NOT green
  Temporal5 — stored temporals canonicalize to ISO strings at the storage
  boundary, so `v.date.year` after read-back returns Null until property reads
  re-derive the typed form. That read-side half belongs to
  phase21_tck-temporal-projection-and-store; the two tasks together complete
  the round-trip.
