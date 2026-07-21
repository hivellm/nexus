# Per-transport reimplementation of query execution (write-path fork)

**Category**: architecture
**Tags**: analysis:nexus-2.5.0, write-path, transport, data-loss, architecture

## Description

Nexus accumulated FIVE divergent write implementations: engine write_exec.rs (correct, tested), HTTP write_ops.rs (1,109-line fork), GraphQL via raw executor (MERGE stubbed as MATCH, SET silently ignored), streaming MCP literal-only CREATE loop, and RPC/RESP3 calling the params-dropping engine.execute_cypher(&str). Every fork produced silent data-loss bugs that the single-path compat suite could not catch (MERGE-rel creates nothing over HTTP, SET r.k dropped, $params stored as null). Root cause: transports were written against low-level engine CRUD methods with string-prefix query routing instead of calling the one tested entry point Engine::execute_cypher_with_params. Full analysis: docs/nexus/04-write-path-unification.md.

## When to Use

Recognize it when: a transport/adapter inspects query text (starts_with) to route; a handler calls low-level CRUD instead of the query pipeline; the same query returns different results on different ports.

## When NOT to Use

Transports may legitimately branch for admin/DB-management commands needing server-level services (DatabaseManager, RBAC) — via a shared AST-predicate helper, never string matching.
