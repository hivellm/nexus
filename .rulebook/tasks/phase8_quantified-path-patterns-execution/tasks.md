## 1. Operator design
- [ ] 1.1 Read `phase6_opencypher-quantified-path-patterns` AST + grammar work
- [ ] 1.2 Spec the `QuantifiedExpand` physical operator (input shape, output shape, traversal algorithm per mode) under `crates/nexus-core/src/executor/operators/`
- [ ] 1.3 Decide BFS vs DFS default; document via `rulebook_decision_create`

## 2. Operator implementation
- [ ] 2.1 Implement `QuantifiedExpand` for mode = WALK
- [ ] 2.2 Implement `QuantifiedExpand` for mode = TRAIL (track visited edges)
- [ ] 2.3 Implement `QuantifiedExpand` for mode = ACYCLIC (track visited nodes)
- [ ] 2.4 Implement `QuantifiedExpand` for mode = SIMPLE (TRAIL + ACYCLIC)
- [ ] 2.5 Carry path variables through to projection (length, nodes, relationships accessors)
- [ ] 2.6 Cost estimate: bound by `n_label * avg_degree^max_hops`; cap explosion via planner threshold

## 3. Planner integration
- [ ] 3.1 Recognise pattern-group quantifier in logical plan
- [ ] 3.2 Lower `((...)-[...]->(...)){n,m}` to `QuantifiedExpand`
- [ ] 3.3 Register the operator with cost-compare against legacy `*n..m` shortest-path

## 4. Tests
- [ ] 4.1 Unit test each mode in isolation
- [ ] 4.2 Edge-case tests: `{0,5}` zero-length, `{2,}` unbounded upper, `{0,0}` empty
- [ ] 4.3 Path-function tests (length, nodes, relationships)
- [ ] 4.4 Run Neo4j diff-suite — confirm 300/300 still passes
- [ ] 4.5 Add Neo4j 5.9+ scenarios to diff-suite if not present

## 5. Documentation
- [ ] 5.1 Update `docs/specs/cypher-subset.md` to mark quantified paths as supported
- [ ] 5.2 Update README openCypher matrix at footer
- [ ] 5.3 CHANGELOG entry

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass
