## 1. Implementation
- [x] 1.1 bracket-less relationship forms parse (`-->`, `--`, `<--`) — anonymous-relationship branch added to parse_relationship_pattern (executor/parser/clauses/pattern.rs); bracketed and var-length forms unchanged
- [x] 1.2 rel-type alternation `:A|:B` parses (parse_types) producing the existing multi-type AST

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation (OPENCYPHER_TCK_REPORT.md regenerated)
- [x] 2.2 Write tests covering the new behavior (tests/cypher/test_bracketless_relationships.rs + parser/tests/patterns.rs)
- [x] 2.3 Run tests and confirm they pass (full nexus-core suite green; clippy/fmt clean; TCK clauses/match 339 -> 332, total 3255 -> 3214)
