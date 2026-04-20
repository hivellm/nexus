# Implementation Tasks — openCypher Quick Wins

## 1. Type-Checking Predicates

- [x] 1.1 Added `isinteger`, `isfloat`, `isstring`, `isboolean` arms in the `FunctionCall` dispatcher at `crates/nexus-core/src/executor/eval/projection.rs`. (Function registry is the inline match-arm block alongside `tointeger`, `tostring`, etc. — no dedicated `functions.rs` exists yet in this tree.)
- [x] 1.2 Added `islist`, `ismap`. `ismap` distinguishes plain user maps from serialised graph entities by checking for the `_nexus_id` marker.
- [x] 1.3 Added `isnode`, `isrelationship`, `ispath`. Node/relationship disambiguated by the presence of a `type` key (relationships carry their relationship-type there); paths recognised as a non-empty Array of `_nexus_id`-tagged Objects.
- [x] 1.4 All nine predicates callable — Cypher's function-name matching is case-insensitive (verified by the regression test hitting `ISINTEGER`, `isinteger`, `isInteger`).
- [x] 1.5 Regression: `type_check_predicates_report_runtime_types` (`crates/nexus-core/src/engine/tests.rs`) covers every builtin scalar + real nodes/relationships + NULL propagation + case-insensitive calls.

## 2. List Type Converters

- [x] 2.1 `tointegerlist`: per-element coercion; elements that fail parse become NULL; non-LIST input raises `TypeMismatch`; NULL input returns NULL (not []).
- [x] 2.2 `tofloatlist`: same per-element + NULL-propagation rules; Bool → 1.0/0.0; NaN/inf floats collapse to NULL.
- [x] 2.3 `tostringlist`: every scalar type serialised via its Display/`to_string`; NULL elements preserved.
- [x] 2.4 `tobooleanlist`: accepts canonical `"true"`/`"false"` (case-insensitive), Bool pass-through, `1`/`0` via f64 non-zero test; unknown strings become NULL.
- [x] 2.5 Four dispatch arms landed in the `FunctionCall` match in `projection.rs`. Regression: `list_converters_is_empty_string_extraction_and_exists`.

## 3. `isEmpty` Polymorphic Predicate

- [x] 3.1 `isempty` arm in `projection.rs` dispatches on `Value::String`, `Value::Array`, `Value::Object`. Graph-entity Objects (carrying `_nexus_id`) always return false; plain maps compare by user-visible key count.
- [x] 3.2 Regression covers all three target types (`isEmpty('')`, `isEmpty([])`, `isEmpty({})`) and their non-empty counterparts.
- [x] 3.3 `isEmpty(null)` returns NULL — locked by the same regression test.

## 4. String Extraction Builtins

- [x] 4.1 `left(str, n)` uses `s.chars().take(n)` — UTF-8-safe per-codepoint slicing, never cuts a multi-byte codepoint.
- [x] 4.2 `right(str, n)` mirrors `left` via `chars()` with a head offset equal to `len - take`.
- [x] 4.3 Negative `n` treated as 0 (returns empty string); `n > len` returns the whole string; NULL string OR NULL `n` returns NULL (three-valued logic).
- [x] 4.4 Regression covers the `n > len` edge and NULL propagation via `list_converters_is_empty_string_extraction_and_exists`.

## 5. Dynamic Property Access `n[expr]`

- [ ] 5.1 Parser: accept `primary '[' expr ']'` as a property reference
- [ ] 5.2 AST: add `AccessKind::Dynamic(Box<Expr>)` to `PropertyAccess`
- [ ] 5.3 Evaluator: coerce key to String, look up property
- [ ] 5.4 Writer: allow `SET n[$k] = v` on the LHS of SET
- [ ] 5.5 Error `ERR_INVALID_KEY` on non-string key expression
- [ ] 5.6 Unit + integration tests

## 6. `SET +=` Map Merge

- [ ] 6.1 Parser: accept `SET lhs += mapExpr` as a distinct `SetItem` variant
- [ ] 6.2 AST: new `SetItem::MapMerge`
- [ ] 6.3 Operator: `SetPropertyMapMerge` that merges but does not remove absent keys
- [ ] 6.4 Contrast with `SET lhs = mapExpr` (replace) — shared test matrix
- [ ] 6.5 Tests: merge + partial overwrite + NULL value handling

## 7. `exists(expr)` Scalar Function

- [x] 7.1 `exists` arm added to the `FunctionCall` dispatcher. Recognises a `PropertyAccess` argument directly so it can tell "absent" from "present but NULL" — plain expression arguments collapse to `!NULL`.
- [x] 7.2 Returns BOOLEAN. Present-and-non-NULL → true; absent or NULL-valued → false; entire target NULL → NULL (three-valued logic).
- [x] 7.3 Disambiguated at `crates/nexus-core/src/executor/parser/expressions.rs` around line 390: `EXISTS` keyword followed by `{` routes to `parse_exists_expression` (pattern predicate); everything else falls through to `parse_identifier_expression`, which emits a `FunctionCall`.
- [x] 7.4 Regression covers present + absent cases (NULL-valued property omitted because the current parser rejects `null` literals inside inline property maps — a distinct parser gap). Lock is in `list_converters_is_empty_string_extraction_and_exists`.

## 8. Read-Only Dynamic Label Lookup

- [ ] 8.1 Parser: accept `n:$param` in WHERE/RETURN positions only
- [ ] 8.2 Evaluator: compile to `any(l IN labels(n) WHERE l = $param)`
- [ ] 8.3 Reject `CREATE (n:$x)` with explicit error (out of scope — see advanced-types task)
- [ ] 8.4 Tests — label match, label miss, unknown label, list param

## 9. TCK Integration

- [ ] 9.1 Add `quickwins_tck.rs` test harness
- [ ] 9.2 Port relevant openCypher TCK scenarios (~120 tests)
- [ ] 9.3 Extend Neo4j diff runner to exercise new features
- [ ] 9.4 Confirm 300/300 existing diff tests still pass
- [ ] 9.5 Verify ≥95% coverage on new modules

## 10. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 10.1 Update `docs/specs/cypher-subset.md` with new functions + syntax
- [ ] 10.2 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` coverage table
- [ ] 10.3 Add CHANGELOG.md entry under "Added"
- [ ] 10.4 Update or create documentation covering the implementation
- [ ] 10.5 Write tests covering the new behavior
- [ ] 10.6 Run tests and confirm they pass
- [ ] 10.7 Run `cargo +nightly fmt --all` and `cargo clippy -- -D warnings`
