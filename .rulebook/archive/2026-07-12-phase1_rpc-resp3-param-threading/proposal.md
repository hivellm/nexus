# Proposal: phase1_rpc-resp3-param-threading

## Why

Bug B6 ([docs/nexus/02-bug-inventory.md](../../../docs/nexus/02-bug-inventory.md)):
the RPC transport (port 15475 — the DEFAULT for every first-party SDK) and the
RESP3 transport silently drop `$params` on write queries. `protocol/rpc/dispatch/cypher.rs:273`
calls `engine.execute_cypher(&query)` (the params-dropping variant) instead of
`execute_cypher_with_params`; RESP3's `run_cypher` receives `_params` and
ignores it. Parameterized writes via SDK default transport lose data — the
same class as the HTTP bug fixed in 2.4.0 (d1b97e62).

## What Changes

Thread the already-decoded parameter map into the engine call in both
adapters. Pure wiring — no new logic. Source of truth strategy in
[docs/nexus/04-write-path-unification.md](../../../docs/nexus/04-write-path-unification.md)
(migration Step 1).

## Impact

- Affected specs: specs/param-threading/spec.md (this task)
- Affected code: `crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs`,
  `crates/nexus-server/src/protocol/resp3/command/cypher.rs`
- Breaking change: NO (fixes silent data loss)
- User benefit: parameterized writes over the SDK-default RPC transport and
  RESP3 actually persist their values.
