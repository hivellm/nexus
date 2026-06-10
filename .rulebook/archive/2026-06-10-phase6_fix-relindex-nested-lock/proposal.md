# Proposal: phase6_fix-relindex-nested-lock

Source: GitHub issue #17 (https://github.com/hivellm/nexus/issues/17)

## Why
`RelationshipIndex::add_relationship`
(`crates/nexus-core/src/cache/relationship_index.rs:112`) acquires five
lock scopes per edge insert and, inside the `stats.write()` block
(lines 164-169), takes `node_index.read()` to compute the approximate
`total_nodes`:

```rust
let mut stats = self.stats.write().unwrap();
stats.total_relationships += 1;
let node_index = self.node_index.read().unwrap(); // nested under stats.write
stats.total_nodes = node_index.len() as u64;
```

This nests `node_index.read()` under `stats.write()`. Any path that holds
`node_index.write()` and then needs `stats.write()` forms a lock-order
cycle (writer-preferred RwLock blocks new readers) — a latent deadlock
under concurrent edge inserts. The `total_nodes` value is explicitly
approximate and does not need the node_index lock.

## What Changes
- Remove the `node_index.read()` from inside the `stats.write()` block.
- Maintain `total_nodes` as an atomic counter incremented when a new
  `node_id` key is first inserted into `node_index` (so stats stay
  accurate without the nested lock).
- Confirm there is no remaining nested cross-field lock acquisition in
  `add_relationship` / `remove_relationship`.

## Impact
- Affected specs: relationship index / concurrency
- Affected code: `crates/nexus-core/src/cache/relationship_index.rs`
- Breaking change: NO
- User benefit: removes a latent deadlock under concurrent edge ingest;
  one fewer lock round-trip per edge.

## Notes
- Audit finding #3. Couples with #18 (a failed/poisoned index update is
  silently swallowed, which a deadlock here would trigger).
