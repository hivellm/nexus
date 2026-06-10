# Proposal: phase6_prefilter-indexed-properties

Source: GitHub issue #21 (https://github.com/hivellm/nexus/issues/21)

## Why
`maintain_indexed_properties` (`crates/nexus-core/src/engine/crud.rs:1293`)
runs on every node create/update: it iterates every property, calls
`catalog.get_key_id(prop_name)` (an LMDB read) per property, then
`property_index.has_index(label_id, key_id)` per (label, key) pair. A node
with 20 properties x 5 labels does 100 `has_index` checks + 20 LMDB reads
per write, with no early exit when none of the node's labels are indexed.
`has_index` is an `Arc<RwLock<HashMap>>` read that can stall under
concurrent `add_property` write locks.

## What Changes
- Maintain a cheap in-memory set of currently-indexed `(label_id, key_id)`
  pairs (or `label_id -> {key_id}`) and pre-filter: skip the whole loop for
  nodes whose labels have no registered index, and only resolve/insert the
  properties that are actually indexed.
- Keep the set in sync with `CREATE INDEX` / `DROP INDEX` (and the #11
  startup rebuild).

## Impact
- Affected specs: indexing / write hot path
- Affected code: `crates/nexus-core/src/engine/crud.rs`
  (`maintain_indexed_properties`), index registry
- Breaking change: NO
- User benefit: lower per-node-write overhead at high create rates; less
  `has_index` read-lock contention.

## Notes
- Audit finding #8 (perf). Complements #15/#16 in cutting per-write cost.
