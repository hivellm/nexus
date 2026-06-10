## 1. Implementation
- [ ] 1.1 Thread the original query string / pre-parsed AST through the legacy `execute_cypher_ast` read fallbacks (or implement a real AST → Cypher serializer for the re-parsed clause set) so no consumer re-parses Debug output
- [ ] 1.2 Fix `PROFILE CALL { ... }` parsing to retain the inner query string ("Query must contain at least one clause" today)

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior (PROFILE over a CALL subquery; legacy path executes an inner MATCH ... RETURN)
- [ ] 2.3 Run tests and confirm they pass
