# Implementation Tasks — Quantified Path Patterns

## 1. Grammar & Tokenization

- [ ] 1.1 Add quantifier tokens `{m,n}`, `{m,}`, `{,n}`, `{n}`, `+`, `*`, `?`
- [ ] 1.2 Disambiguate from map literals (`{a:1}`) via lookahead
- [ ] 1.3 Extend pattern grammar to accept `'(' pathFragment ')' quantifier`
- [ ] 1.4 Allow nested QPP (one level deep — Cypher 25 restriction)
- [ ] 1.5 Reject empty quantifier `{}`
- [ ] 1.6 Parser unit tests for every quantifier form

## 2. AST & Type Promotion

- [ ] 2.1 Add `PatternPart::Quantified(Box<PathFragment>, Quantifier)` to AST
- [ ] 2.2 Promote inner pattern variables to LIST type in outer scope
- [ ] 2.3 Preserve ordering semantics: `x[0]` is the first iteration
- [ ] 2.4 Handle zero-length case (`{0,n}`): inner lists are empty
- [ ] 2.5 Tests asserting type promotion

## 3. Planner Operator: QuantifiedExpand

- [ ] 3.1 Create `operators/quantified_expand.rs`
- [ ] 3.2 Inner sub-plan runs once per iteration with scoped bindings
- [ ] 3.3 Backtracking search with per-frame bookkeeping
- [ ] 3.4 Cycle policy: MATCH uses NODES_CAN_REPEAT, ALL_DIFFERENT for shortestPath
- [ ] 3.5 Enforce pattern lower/upper bounds
- [ ] 3.6 Emit inner variables as LIST values on successful match

## 4. Cost Model

- [ ] 4.1 Cost estimate ≈ (avg_inner_fanout)^k × outer_rows for QPP of length k
- [ ] 4.2 Prefer index-backed starting nodes before entering the quantified body
- [ ] 4.3 Push inner `WHERE` predicates into the iteration scope
- [ ] 4.4 Planner tests asserting expected operator order

## 5. `shortestPath` / `allShortestPaths` Integration

- [ ] 5.1 Accept quantified patterns as the path argument
- [ ] 5.2 BFS on quantified iterations uses iteration count as cost
- [ ] 5.3 Early termination once a shortest match is confirmed
- [ ] 5.4 Tests covering shortestPath over quantified patterns

## 6. Rewriter for Legacy Variable-Length Paths

- [ ] 6.1 Rewrite `-[r:TYPE*m..n]->` to quantified form internally
- [ ] 6.2 Preserve user-facing type: `r` stays LIST<RELATIONSHIP>
- [ ] 6.3 Regression: every existing `*m..n` query plan must still succeed
- [ ] 6.4 Tests: identical plans for rewrites vs hand-written QPP equivalents

## 7. Error Taxonomy

- [ ] 7.1 `ERR_QPP_UNBOUND_UPPER`: `*` without explicit cap rejected when memory would blow up
- [ ] 7.2 `ERR_QPP_NESTING_TOO_DEEP`: reject nesting > 1 level
- [ ] 7.3 `ERR_QPP_INVALID_QUANTIFIER`: reject `{n,m}` where `n > m`
- [ ] 7.4 Error message tests with position spans

## 8. openCypher TCK Coverage

- [ ] 8.1 Import openCypher TCK QPP features (`quantified-path-patterns/*`)
- [ ] 8.2 Tag unsupported scenarios with `@qpp-scope` exclusions
- [ ] 8.3 Run TCK in CI; target 95%+ pass on in-scope scenarios
- [ ] 8.4 Compare output against Neo4j 5.15 diff harness

## 9. Performance Benchmarks

- [ ] 9.1 Bench: 5-hop friend-of-friend over 10k nodes
- [ ] 9.2 Bench: bounded reachability (`{1,5}`) vs legacy `*1..5`
- [ ] 9.3 Bench: worst-case cycle-free traversal depth 10
- [ ] 9.4 Regression: new ops must stay within 1.3× legacy runtime

## 10. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 10.1 Update `docs/specs/cypher-subset.md` with the new grammar
- [ ] 10.2 Add `docs/guides/QUANTIFIED_PATH_PATTERNS.md` user guide
- [ ] 10.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- [ ] 10.4 Add CHANGELOG entry "Added quantified path patterns (Cypher 25)"
- [ ] 10.5 Update or create documentation covering the implementation
- [ ] 10.6 Write tests covering the new behavior
- [ ] 10.7 Run tests and confirm they pass
- [ ] 10.8 Quality pipeline: fmt + clippy + ≥95% coverage
