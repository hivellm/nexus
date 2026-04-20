# Implementation Tasks — openCypher Quick Wins

## 1. Type-Checking Predicates

- [ ] 1.1 Add `is_integer`, `is_float`, `is_string`, `is_boolean` to `executor/eval/functions.rs`
- [ ] 1.2 Add `is_list`, `is_map` with element-type awareness
- [ ] 1.3 Add `is_node`, `is_relationship`, `is_path` (graph type checks)
- [ ] 1.4 Register all 9 predicates in the function registry
- [ ] 1.5 Unit tests covering every builtin scalar type

## 2. List Type Converters

- [ ] 2.1 Implement `toIntegerList(list)` with NULL-propagation on failure
- [ ] 2.2 Implement `toFloatList(list)`
- [ ] 2.3 Implement `toStringList(list)`
- [ ] 2.4 Implement `toBooleanList(list)` accepting `"true"/"false"/1/0`
- [ ] 2.5 Add registry entries and unit tests

## 3. `isEmpty` Polymorphic Predicate

- [ ] 3.1 Implement `is_empty` dispatching on `String`, `List`, `Map`
- [ ] 3.2 Register and test against all three target types
- [ ] 3.3 Document NULL behaviour (`isEmpty(NULL) = NULL`)

## 4. String Extraction Builtins

- [ ] 4.1 Implement `left(str, n)` — first `n` chars (UTF-8 safe)
- [ ] 4.2 Implement `right(str, n)` — last `n` chars
- [ ] 4.3 Handle negative `n`, `n > len(str)`, NULL inputs
- [ ] 4.4 Unit tests with multi-byte characters

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

- [ ] 7.1 Add scalar `exists(n.prop)` distinct from `EXISTS { pattern }` predicate
- [ ] 7.2 Returns BOOLEAN (true if property present, false otherwise)
- [ ] 7.3 Disambiguate from pattern EXISTS in the parser (token lookahead)
- [ ] 7.4 Tests covering present / absent / NULL-valued properties

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
