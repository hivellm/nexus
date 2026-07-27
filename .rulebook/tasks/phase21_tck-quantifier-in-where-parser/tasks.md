## 1. Implementation
- [x] 1.1 any/all/none/single parse the `(x IN list WHERE pred)` form (WHERE required), mirroring the filter() special form; produces `FunctionCall(name, [String(varname), list, predicate])` matching the fn_list.rs evaluators. Verified in a WHERE clause (the task's target). Parser change in executor/parser/expressions/identifier.rs.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation (module doc in test_quantifier_predicates.rs + memory note on the bare-RETURN projection follow-up; removed non-standard variadic any(null,...) noted for phase CHANGELOG)
- [x] 2.2 Write tests covering the new behavior (tests/cypher/test_quantifier_predicates.rs: WHERE-context any/all/none/single + WHERE-required parse error; removed 4 non-standard variadic tests in new_functions_test.rs)
- [x] 2.3 Run tests and confirm they pass (cypher target 405/0, full nexus-core 0 targets failed, clippy/fmt clean; TCK expressions/quantifier 588 -> 563)

## Known follow-up (NOT this task)
- Bare `RETURN any(x IN list WHERE pred)` projection drops the row (planner hoists the inner bound
  var as an external column). Documented `#[ignore]`d test + memory
  `project-planner-hoists-quantifier-bound-var-in-projection`. Belongs with
  phase21_tck-semantic-analysis-pass / a projection-scoping fix; it (plus 3VL) gates most of the
  remaining quantifier scenarios.
