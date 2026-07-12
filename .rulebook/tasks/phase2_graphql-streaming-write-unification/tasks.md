## 1. GraphQL mutations
- [ ] 1.1 Write red tests first: GraphQL mutation with MERGE persists the node/rel; mutation with SET persists the property (currently both silently no-op)
- [ ] 1.2 Route mutating resolvers in `api/graphql/mutation.rs` through `engine.execute_cypher_with_params`; keep read resolvers on the lock-free executor
- [ ] 1.3 Sweep `api/graphql/resolver.rs` for any other write-clause execution via raw executor and route those too

## 2. Streaming MCP handler
- [ ] 2.1 Write red tests: streaming CREATE with `$param` property persists the value (currently null)
- [ ] 2.2 Delete the literal-only CREATE loop in `api/streaming/handlers.rs::handle_execute_cypher`; call `engine.execute_cypher_with_params`

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the implementation (CHANGELOG "Fixed"; docs/nexus/02 elimination map ticked)
- [ ] 3.2 Write tests covering the new behavior (the red tests from 1.1/2.1 now green + parity battery run over GraphQL and streaming)
- [ ] 3.3 Run tests and confirm they pass (`cargo +nightly test -p nexus-server`, clippy zero warnings)
