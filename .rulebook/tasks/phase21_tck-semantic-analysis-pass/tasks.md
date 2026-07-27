## 1. Implementation
- [ ] 1.1 variable scoping validation stage
  - PROGRESS (commit 0ac36d07): module `executor/semantic_validation.rs` + hook in `engine/query_pipeline.rs::execute_cypher_with_context` (after parse, before plan). `UndefinedVariable` DONE — over-collects binders (pattern node/rel/path/QPP vars, WITH/RETURN aliases, UNWIND/FOREACH vars, list/pattern-comprehension + any/all/none/single predicate vars), flags a reference bound nowhere; bails on UNION/CALL{}/CALL-proc/LOAD CSV/DDL; excludes the synthetic `__DISTINCT__` marker; bare RETURN/WITH items are references not binders. Full nexus-core suite green (4377/0), 7 unit tests in-module.
  - REMAINING for 1.1: `VariableTypeConflict` (name bound as node AND as rel — collect node-bound vs rel-bound name sets, intersection = conflict), `VariableAlreadyBound` (rebinding an already-bound name to a NEW entity, e.g. `UNWIND [1] AS a MATCH (a)`; be conservative — same-entity reuse in one MATCH is legal), `NoVariablesInScope` (`RETURN *`/`WITH *` with empty scope). Each needs the full-suite gate (false-positive = valid-query regression = the plan's #1 risk).
- [ ] 1.2 aggregation placement validation
- [ ] 1.3 SKIP/LIMIT arg validation
- [ ] 1.4 projection/UNION/write structural validation
- [ ] 1.5 OpenCypherErrorKind + detail tokens emitted

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
