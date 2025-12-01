# Implement GraphQL API

## Why

GraphQL provides a modern, flexible query language that allows clients to request exactly the data they need, reducing over-fetching and under-fetching common in REST APIs. Many modern applications prefer GraphQL for its type-safe queries, introspection capabilities, and efficient data fetching. Adding GraphQL support to Nexus will make it more accessible to developers familiar with GraphQL ecosystems and enable better integration with GraphQL tooling and frameworks.

## What Changes

- Add GraphQL endpoint (`/graphql`) to nexus-server
- Implement GraphQL schema generation from graph database schema (labels, relationship types, properties)
- Implement GraphQL query resolver that translates GraphQL queries to Cypher queries
- Support GraphQL mutations for creating/updating/deleting nodes and relationships
- Support GraphQL subscriptions for real-time updates (optional, future enhancement)
- Add GraphQL introspection support for schema discovery
- Maintain compatibility with existing REST API (both APIs coexist)

**BREAKING**: None (additive feature, REST API remains unchanged)

## Impact

### Affected Specs
- NEW capability: `graphql-api`

### Affected Code
- `nexus-server/src/api/graphql/` - New module (~1500 lines)
  - `schema.rs` - GraphQL schema generation from database schema
  - `resolver.rs` - Query/mutation resolvers
  - `cypher_translator.rs` - GraphQL to Cypher query translation
  - `types.rs` - GraphQL type definitions
- `nexus-server/src/main.rs` - Add GraphQL route
- `nexus-server/Cargo.toml` - Add async-graphql dependency

### Dependencies
- Requires: Core graph engine, Cypher query execution, Schema management
- New dependency: `async-graphql` crate for GraphQL server implementation

### Timeline
- **Duration**: 4-5 weeks
- **Complexity**: High (GraphQL schema generation, query translation, type system)
