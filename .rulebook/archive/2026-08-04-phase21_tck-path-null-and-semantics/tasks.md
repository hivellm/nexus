## 1. Implementation

> **The proposal's diagnosis was wrong and the checklist below is corrected.**
> It predicted the whole category came down to 1.1 + 1.2. Reading the
> instrumented failure log (`target/tck-failures.jsonl`) against the three
> feature files showed 3 of the 7 scenarios were a different, larger defect that
> the proposal never mentioned — recorded as 1.3. Item 1.1 also needed a
> column-name fix it did not mention (the value assertion was never reached,
> because `compare_table` checks column names first and aborts).

- [x] 1.1 nodes/relationships null-safety
  - `nodes(null)` / `relationships(null)` returned `[]`, and `length(null)`
    returned `0`, because a null argument matched neither the array nor the
    object branch and fell through to an empty/zero default. All three now
    propagate null. This also covers the `OPTIONAL MATCH p = …` no-match case,
    where `p` itself is null — `[]` there wrongly reads as "a real path that
    happens to be empty".
  - Second, unlisted blocker on the same two scenarios: the unaliased column
    name rendered as `nodes(NULL)` where the TCK compares against the source
    text `nodes(null)`. `Literal::Null` in the planner's `expression_to_string`
    now renders lower-case, matching how `Literal::Boolean` beside it already
    rendered. The string is only otherwise re-parsed as a WHERE predicate, where
    the keyword match is case-insensitive. (The AST carries no source span, so
    this is canonical lower-case, not a true verbatim echo of the author's
    casing — the same limitation booleans already had.)
- [x] 1.2 length() type errors
  - New `check_length_argument_type` pass in `executor/semantic_validation.rs`
    rejects `length()` over a variable the query binds as a node or a
    relationship with `SyntaxError` / `InvalidArgumentType` at compile time,
    reusing the node/relationship variable sets the pass already collected for
    `check_variable_type_conflicts` (extracted as `typed_pattern_vars`).
  - Only a directly-named pattern variable is judged — that is where the type is
    statically known. A property access, function result, parameter, `WITH` alias
    or path variable is left to runtime, honouring the module's documented
    no-false-positive rule. A name bound as both node and relationship is skipped
    too: that is `VariableTypeConflict`, reported earlier by `validate`.
  - To avoid duplicating clause-position knowledge, the read-position
    enumeration was extracted as `clause_read_exprs` and is now shared by this
    check and `check_clause_references`. Sub-expression recursion reuses the
    existing `child_exprs` helper (a first draft duplicated it; removed).
- [x] 1.3 Variable-length paths must carry their relationships **(not in the
      original proposal — the largest of the three defects)**
  - `Operator::VariableLengthPath` bound the path variable to a nodes-only list
    (`operators/path.rs`), so `relationships(p)` was empty and `length(p)` zero
    for every variable-length match. The path value is now the alternating
    `[n0, r0, n1, r1, …, nN]` sequence the path functions expect; BFS already
    kept `path_nodes[i] -[path_rels[i]]-> path_nodes[i+1]` aligned. Only the
    `_with_store` readers are used inside the held guard — `parking_lot`'s
    RwLock is not reentrant and re-acquiring would deadlock.
  - Interleaving alone was not enough: `relationships()` and `length()` filtered
    on `_nexus_type` plus `_source`/`_target`, **none of which exist** on a
    relationship value (the marker is `_nexus_rel_type`), so they matched nothing
    on any path; and `nodes()` filtered on a bare `_nexus_id`, which
    relationships also carry, so it counted them as nodes. All three now use the
    canonical `is_node_value` / `is_relationship_value` helpers — the project's
    already-recorded rule for this exact trap.

### Deliberate behaviour change to an asserted contract

`tests/cypher/builtin_functions_test.rs::test_path_functions_with_null` asserted
`nodes(null) == []`, `relationships(null) == []` and `length(null) == 0`. The
openCypher contract is `null` for all three, so that assertion was updated. It
is the old contract being corrected, not a test bent to fit the code.

### Adjacent pre-existing bug fixed

`length('hello')` returned `0` although `docs/specs/cypher-subset.md` documents
it under the string functions. Surfaced by a control test written for 1.2; it
falls in the same `length` arm, so it is fixed here (character count).

### Residuals deliberately NOT fixed (each needs its own task)

- `size(string)` counts UTF-8 bytes while `length(string)` now counts
  characters — the two disagree on non-ASCII input. `size()` is the wrong one.
  Recorded in the spec as a known divergence.
- Two path representations coexist: the array shape above, and the
  `{nodes: […], relationships: […]}` object `path_to_value` builds for
  `shortestPath`/`allShortestPaths` and pattern comprehensions. The path
  functions only read the array shape, so `nodes(shortestPath(…))` does not
  work. Pre-existing and unaffected by this change (an object without
  `_nexus_id` fell through to the same empty default before and after); no
  scenario, test or doc exercises it.
- A path variable on a **non**-variable-length pattern (`MATCH p = (a)-[r]->(b)`)
  is never bound at all: `pattern.path_variable` is consumed only by the
  `VariableLengthPath` lowering in `planner/queries/relationships.rs`. Outside
  this task's scenarios, all of which are variable-length.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md` § Path Functions: path value representation
    (including the zero-length case), null propagation, the `length()` argument
    type contract with examples, and the `size()`/`length()` divergence note.
  - `docs/compatibility/OPENCYPHER_TCK_REPORT.md` regenerated by the TCK run.
- [x] 2.2 Write tests covering the new behavior
  - 11 tests in `tests/cypher/path_function_semantics_test.rs`: null path for
    `nodes`/`relationships` (asserting the column names too), var-length
    `relationships(p)` from both anchor ends, `length(p)` over `*0..1` including
    the zero-length rows, `length()` type errors on node / relationship /
    nested-in-an-expression, and controls proving the pass leaves
    `length('abcd')`, `length(n.name)` and `nodes(p)` alone.
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher`: 625 passed / 0 failed / 6 ignored.
  - `--test executor`: 272 passed / 0 failed.
  - Full `--workspace --no-fail-fast`: see the commit message for the tally.
  - `cargo +nightly fmt --all` + `cargo clippy --all-targets --all-features
    -- -D warnings`: clean.
  - **TCK: `expressions/path` 0/7 → 7/7 (100%)**, zero path entries left in the
    failure log. Shared-code spillover in the categories that consume path
    values and the entity-kind markers: `expressions/quantifier` +4,
    `expressions/graph` +3, `expressions/list` +1. Overall 1800 → 1817 of 3868
    (46.5% → 47.0%). `clauses/merge` +1 and `clauses/with-orderBy` +1 are inside
    the documented ±4 noise band.
  - The two `exists_*_var_length` lib tests fail only in full-parallel runs and
    pass isolated. They touch the variable-length code this task changed, so
    they were re-run isolated rather than assumed: `2 passed; 0 failed`. They
    also failed identically in the pre-change baseline run.
