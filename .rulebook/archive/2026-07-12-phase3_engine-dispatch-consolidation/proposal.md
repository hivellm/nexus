# Proposal: phase3_engine-dispatch-consolidation

## Why

Latent bugs L4 and L5 ([docs/nexus/02-bug-inventory.md](../../../docs/nexus/02-bug-inventory.md)):

- `engine/query_pipeline.rs` contains **two near-identical ~200-line dispatch
  functions** — `execute_cypher_dispatch` and `execute_cypher_ast` (the
  EXPLAIN/PROFILE path). Fixes applied to one silently miss the other, so
  PROFILE results can drift from real execution.
- `Engine::execute_cypher(&str)` **silently drops parameters** — the exact
  footgun that caused bugs B4 (HTTP, fixed 2.4.0) and B6 (RPC/RESP3). Any
  future caller of the convenient-looking method reintroduces the bug class.

Migration Step 7 of
[docs/nexus/04-write-path-unification.md](../../../docs/nexus/04-write-path-unification.md).

## What Changes

- Unify both dispatch functions into one private `dispatch(ast, query, opts)`
  where EXPLAIN/PROFILE and internal callers pass a flag.
- Retire the params-dropping API: `execute_cypher(&str)` either takes params
  or delegates to `execute_cypher_with_params(query, HashMap::new())` with a
  doc comment steering callers; audit all internal call sites.

## Impact

- Affected specs: specs/engine-dispatch/spec.md (this task)
- Affected code: `crates/nexus-core/src/engine/query_pipeline.rs` and every
  internal `execute_cypher` call site
- Breaking change: NO for external SDK users (HTTP/RPC unchanged); internal
  API signature may change (workspace-internal)
- User benefit: PROFILE output guaranteed consistent with real execution; the
  params-dropping bug class becomes unrepresentable.
