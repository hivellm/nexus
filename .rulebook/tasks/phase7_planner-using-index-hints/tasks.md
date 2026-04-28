## 1. Implementation
- [ ] 1.1 Locate `UsingHint` AST nodes in parser output
- [ ] 1.2 Thread hints from logical plan into physical-operator selection in `executor/planner/`
- [ ] 1.3 Implement `USING INDEX` directive (force named index; error if missing)
- [ ] 1.4 Implement `USING SCAN` directive (force label scan, bypass index selection)
- [ ] 1.5 Implement `USING JOIN ON v` directive (force hash-join on named variable)
- [ ] 1.6 Add planner unit tests: each hint shape isolated
- [ ] 1.7 Add conflict test: hint vs cost-model preference — hint wins
- [ ] 1.8 Add error test: `USING INDEX :L(missing)` fails with clear message
- [ ] 1.9 Update `docs/specs/cypher-subset.md` to mark hints as supported
- [ ] 1.10 Run Neo4j diff-suite — confirm 300/300 still passes

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
