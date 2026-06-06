# Proposal: phase6_fix-cypher-param-binding

Source: GitHub issue #3 (https://github.com/hivellm/nexus/issues/3)

## Why
`POST /cypher` silently ignores `$param` placeholders when a `parameters`
block is supplied. Every parametrized query returns `rows: []` with no
error, so callers cannot detect the failure programmatically — only by
comparing against the inlined form. This breaks the canonical SDK API
`execute_cypher(query, Some(params))`; any caller threading user input
through parameters retrieves zero rows. Reproduced on `hivehub/nexus:2.2.0`
against a 137k-node graph. Downstream (`hivellm/cortex`
`cortex_graph_query`) had to inline sanitized ids as a workaround.

## What Changes
- Fix parameter binding so `$id` placeholders are substituted from the
  `parameters` block before / during execution, in both forms:
  - inline map pattern: `MATCH (s {id: $id}) RETURN id(s)`
  - WHERE predicate: `MATCH (s) WHERE s.id = $id RETURN id(s)`
- Both must return the same rows as the equivalent inlined-literal query.
- Audit the parser -> planner -> executor path to confirm parameter values
  reach predicate evaluation (property-map match and WHERE comparison).
- Ensure a binding failure (missing param referenced by query) surfaces a
  structured error instead of empty rows.

## Impact
- Affected specs: cypher-subset / parameters
- Affected code: `crates/nexus-core/src/executor/` (parameter resolution,
  predicate evaluation); `crates/nexus-server/src/api/` (cypher handler
  parameter plumbing)
- Breaking change: NO (fixes broken behavior; response format unchanged)
- User benefit: parametrized queries work across all SDKs; no inline
  workarounds, no injection-prone string interpolation
