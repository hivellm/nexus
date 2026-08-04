## 1. Implementation
- [x] 1.1 Move the `semantic_validation::validate` call from `execute_cypher_with_context` (query_pipeline.rs:186) into the shared body `execute_cypher_ast_with_context`, so `execute_cypher_ast_with_params` (the RPC/Thunder write path) is covered — and confirm the HTTP path does not validate twice
  - Moved. It runs at the top of the shared body, **before** the cluster-mode scope
    rewrite, so the checks and their messages see the names the user wrote rather
    than the tenant-prefixed form.
  - No double validation on the HTTP path: `execute_cypher_with_context` no longer
    validates itself, it only parses and delegates.
- [x] 1.2 Audit the RPC read-only autocommit fast path that calls the lock-free executor directly, bypassing `Engine`: either route it through validation or document why it cannot, without reintroducing a re-parse
  - **The proposal's premise was wrong on one material point, and measuring proved
    it.** It stated "HTTP `/cypher` goes through `execute_cypher_with_params` and
    *is* validated". It is not: `api::cypher::execute::handler` has the SAME
    lock-free read carve-out as the RPC dispatcher (handler.rs:668-703, which the
    RPC comment even points at as the thing it mirrors), so a pure read bypassed
    `Engine` — and validation — on BOTH transports. The transport-dependent
    behaviour the proposal describes was real, but the split was shape-dependent
    too, not transport-only.
  - Verified by A/B rather than argued: with the executor-side check disabled in
    place, the parity test fails on `RETURN b` **over HTTP**. That is the direct
    measurement that the HTTP read path was unvalidated.
  - Routed through validation at `executor::dispatch::planning::parse_and_plan`,
    on the AST that call just parsed — so no re-parse, which was the constraint.
    That one site covers both transports' fast paths, `EXPLAIN`/`PROFILE` internal
    re-execution, and the engine's executor-fallback branches.
  - Deliberately NOT in `plan_ast`: the cluster-mode handoff lands there with an
    AST the engine already validated and then rewrote for tenant scoping.
  - Both sites are needed, and neither is redundant: the engine's write path
    (`MERGE`/`SET`/`REMOVE`/`FOREACH` via `execute_write_query`) builds its own
    result without going through the executor's parse, and the lock-free path
    never reaches the engine. A query that is engine-intercepted AND reaches the
    executor pays one extra AST walk; that is a pure-function walk on a path
    dominated by storage I/O, and it buys the guarantee that no future entry point
    can silently skip the pass.
- [x] 1.3 Transport-parity test: the same semantically invalid query (one per check — undefined variable, variable type conflict, already-bound variable, misplaced aggregation, negative SKIP/LIMIT, duplicate alias) is rejected with the same error kind over HTTP `/cypher` and over the RPC `CYPHER` command
  - `api/cypher/semantic_validation_parity.rs`, 7 invalid queries (the six asked
    for plus `RelationshipUniquenessViolation`), each asserted rejected with the
    same detail token on both surfaces, plus a mirror-image test that 4 VALID
    queries still succeed on both — without it, a pass that rejected everything
    would satisfy the first test.
  - Both surfaces are driven as functions against ONE server: the HTTP handler
    directly (as `write_path_parity` does) and the RPC dispatcher's `run` with a
    hand-built `RpcSession`. No socket, so no transport framing in the way, and the
    two surfaces are guaranteed to be looking at the same engine state.
  - The query set deliberately spans both bypass shapes: the bare
    `RETURN`/`MATCH … RETURN` cases take the lock-free read carve-out, while the
    `CREATE` case goes through the engine's pre-parsed-AST write path.
  - **Two of the queries first written for this test were wrong, and the test
    caught it, not the reverse.** `MATCH (a) CREATE (a)-[:T]->(b)` is legal Cypher
    (using a bound variable as an endpoint is the normal way to attach an edge),
    and the bare `MATCH (a) CREATE (a)` form is a deliberate carve-out in
    `check_create_element_rebind` (it requires `has_structure`). The asserted shape
    is now the TCK's own: `MATCH (a) CREATE (a {name: 'foo'})`.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md` § Semantic Validation: states that the pass
    applies to every transport and entry point, names the two sites and what each
    covers, and records that this was not previously true. Also adds the
    `RelationshipUniquenessViolation` row, which was missing from the token table.
  - `CHANGELOG.md`: the observable behavior change for binary-transport clients is
    called out as BREAKING, with the list of checks that will start firing for RPC
    clients — the proposal explicitly asked for this.
- [x] 2.2 Write tests covering the new behavior
  - The parity harness above (2 tests, 7 checks × 2 transports + 4 valid queries ×
    2 transports).
  - **Four existing tests had to be updated, and they are the clearest evidence
    the gap was real.** `pattern_comprehension_tests`'s parser-backtracking tests
    used queries with free variables and drove them through the low-level
    `Executor` *because* it had no validation — their own doc comments said so
    ("This pins PARSER behavior only, via the low-level `Executor` (no
    semantic-validation pass)… through the full engine pipeline this same query
    would be rejected with `UndefinedVariable`"). They now bind those variables to
    `null` in the `WITH`, which keeps every assertion identical (`NULL = NULL` and
    `NULL = 3` are both `NULL`) while making the queries valid on every entry
    point. The parser fallback is still what is under test.
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 655, `--test executor` 272, `--test regression` 227,
    `--test compatibility` 245, `nexus-server --lib` 593 — all 0 failures.
    Workspace `--no-fail-fast`: **5792 passed, 3 failed**, the 3 being the
    documented flakes (`property_index_survives_restart`, the two `exists_`
    var-length tests). `fmt` and `clippy --workspace --all-targets --all-features
    -D warnings` clean.
  - **TCK: exactly zero delta — every category identical, total 1827 both sides.**
    That is the expected result, not a disappointment: the conformance harness
    drives `Engine::execute_cypher`, the text-parsing entry point that was already
    validated, so this task can only change what OTHER transports do. There is no
    gain to attribute and no drop to investigate.
  - Found but not fixed (out of scope, worth its own task): the bare
    `MATCH (a) CREATE (a)` re-declaration is still accepted —
    `check_create_element_rebind` requires labels/properties/external-id before it
    flags a rebind. `docs/specs/cypher-subset.md` already lists it under "Not yet
    detected", and the openCypher TCK has a scenario for it in
    `clauses/create/Create1.feature`.
