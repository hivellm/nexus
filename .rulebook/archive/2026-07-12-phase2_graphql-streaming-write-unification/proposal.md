# Proposal: phase2_graphql-streaming-write-unification

## Why

Bugs B5 and B7 ([docs/nexus/02-bug-inventory.md](../../../docs/nexus/02-bug-inventory.md)):

- **GraphQL mutations call the raw executor** (`server.executor.execute`),
  where the planner stubs `Clause::Merge` as a plain MATCH
  (`planner_core.rs:415`) and has NO Set/Remove/Foreach operators — so GraphQL
  MERGE silently matches-only and SET/REMOVE/FOREACH are silently ignored.
  Data loss with HTTP 200.
- **The streaming MCP handler** (`api/streaming/handlers.rs` ~159–228) has a
  5th hand-rolled CREATE mini-fork supporting literal properties only —
  `$params` and non-literal expressions become null.

The correct write implementation exists at
`Engine::execute_cypher_with_params`. Migration Steps 5–6 of
[docs/nexus/04-write-path-unification.md](../../../docs/nexus/04-write-path-unification.md).

## What Changes

- GraphQL mutating resolvers (`api/graphql/mutation.rs`, mutating paths in
  `resolver.rs`) call `engine.execute_cypher_with_params` instead of the raw
  executor. Read resolvers stay on the lock-free executor path.
- Streaming MCP `handle_execute_cypher` deletes the literal-only CREATE loop
  and calls the engine.

## Impact

- Affected specs: specs/transport-writes/spec.md (this task)
- Affected code: `crates/nexus-server/src/api/graphql/mutation.rs`,
  `crates/nexus-server/src/api/graphql/resolver.rs`,
  `crates/nexus-server/src/api/streaming/handlers.rs`
- Breaking change: NO (fixes silent data loss; GraphQL mutations that
  previously no-opped now persist)
- User benefit: GraphQL MERGE/SET and streaming CREATE behave identically to
  every other transport.
