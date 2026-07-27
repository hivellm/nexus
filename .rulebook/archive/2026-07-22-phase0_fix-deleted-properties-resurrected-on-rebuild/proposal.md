# Proposal: phase0_fix-deleted-properties-resurrected-on-rebuild

**Priority: HIGH — deleted properties can come back to life after a restart.**
Found by code review during `phase0_fix-property-store-shrink-corruption`; not
previously reported.

## Why

`PropertyStore::delete_properties`
(`crates/nexus-core/src/storage/property_store.rs`, ~`:382-387`) removes the
entity only from the IN-MEMORY `index`/`reverse_index` — it never tombstones or
zeroes the entity's bytes on disk. The entity's property blob remains a
well-formed, fully-parseable entry. On the next reopen, the index rebuild
(`rebuild_index` / `ensure_index_populated`) scans every parseable entry and
reinserts `(entity_id, entity_type) -> offset`, resurrecting the "deleted"
properties.

This was latent before `phase0_fix-property-store-shrink-corruption` (a
mis-strided scan could accidentally skip the orphan), but that fix made the
rebuild scan reliably recover EVERY parseable on-disk entry, so the
resurrection is now deterministic.

### Trigger (confirmed by code inspection)

```
CREATE (a:Person {secret: 'x'})   -- writes a's property blob
-- delete a's properties (REMOVE all, or delete the node) --
-- restart / reopen the store --
MATCH (a) ... RETURN a.secret     -- 'x' is back: rebuild re-parsed a's intact bytes
```

## What Changes

- On `delete_properties`, tombstone the on-disk entry so the rebuild scan does
  not re-index it — e.g. clear the header to a reserved "dead" marker that the
  shared scanner recognises and skips (while still striding over it correctly).
- Teach the shared rebuild scanner
  (`scan_entry_at`/`try_parse_entry`/`resync_to_next_entry`) to treat a
  tombstoned entry as dead space: stride over it, never insert it.
- Back-compat: existing stores contain deleted entities that were never
  tombstoned; a rebuild must not resurrect them. Reconcile against the
  authoritative record store (a property blob whose owning node/relationship
  record is deleted or absent must not be re-indexed) on rebuild.

## Impact

- Affected specs: `docs/specs/storage-format.md` (property entry — tombstone/
  dead-record encoding and the rebuild skip contract)
- Affected code: `crates/nexus-core/src/storage/property_store.rs`
  (`delete_properties`, `scan_entry_at`/`try_parse_entry`, `rebuild_index`,
  `ensure_index_populated`)
- Breaking change: on-disk gains a tombstone encoding; needs a rebuild/reconcile
  path for existing stores
- User benefit: deleted properties stay deleted across restarts
- Related: `phase0_fix-property-store-shrink-corruption` (same rebuild scanner;
  this closes the resurrection gap that fix's reliable recovery exposed),
  `phase0_fix-update-node-index-divergence`
