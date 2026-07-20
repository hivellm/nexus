# Proposal: phase0_fix-property-store-shrink-corruption

**Priority: CRITICAL — shrinking any node's or relationship's stored
properties (a shorter SET, or REMOVE) silently corrupts an unrelated
entity's properties the next time the store is reopened.** Found during a
write-path/index corruption audit; not previously reported.

## Why

Each entity's properties are stored as one contiguous blob: a 13-byte
header (`entity_id: u64`, `entity_type: u8`, `data_size: u32`) followed by
`data_size` bytes of JSON. `PropertyStore::update_properties`'s in-place
branch handles a shrink (new data no larger than the old) by overwriting
the header's `data_size` and the leading bytes, but never touches the freed
tail:

```rust
// storage/property_store.rs:307-312
if new_data_size <= existing_data_size {
    self.write_u32(offset + 9, new_data_size);
    self.write_bytes(offset + 13, &serialized);
    Ok(offset) // Return same offset
}
```

The bytes from `offset+13+new_data_size` to `offset+13+existing_data_size`
— the tail of the OLD, longer payload — are left on disk untouched. The
entity that physically follows this one on disk keeps its original offset;
nothing about its own header changes. In memory, `self.index`/
`self.reverse_index` still map every entity to its correct offset, so reads
against the LIVE, already-open store are unaffected.

The corruption surfaces on reopen. `file_existed` seeds `next_offset = 0`,
so the full-scan rebuild runs (`rebuild_index`, `:539-599`, and the
fresh-store variant `ensure_index_populated`, `:723-756`). Both scanners
stride strictly by the CURRENT stored `data_size`:

```rust
// property_store.rs:580 (rebuild_index) / :746 (ensure_index_populated)
let entry_size = 8 + 1 + 4 + data_size as usize;   // uses the (possibly shrunk) data_size
...
offset += entry_size as u64;
```

After a shrunk entry, `entry_size` is computed from the SMALLER post-shrink
`data_size`, but the entity that physically follows it still sits at its
ORIGINAL (pre-shrink) offset. The scanner therefore advances to
`offset + (new, smaller) entry_size`, landing INSIDE the stale leftover
tail bytes of the shrunk entry — not at the true start of the next entity's
header. It reads a garbage `entity_type` byte from that stale data. If the
byte doesn't parse as a valid `EntityType`, the scan **breaks early**
(`Err(_) => break`), silently dropping every entity that came after the
shrunk one from `reverse_index` and leaving `next_offset` pointed mid-file.
If the garbage byte happens to parse as valid, the scan continues with a
fabricated (wrong) entity mapping instead.

### Consequence (confirmed by code inspection)

Once `next_offset` is wrong, the next property write (`store_properties`/
`update_properties`'s grow branch) allocates new space starting at that
wrong `next_offset` — which lands inside or before a still-live entity's
blob — and overwrites it. The victim entity then returns wrong, truncated,
or cross-entity-typed properties. This is exactly the shape the existing
corruption-warning fallback in `record_store_ops.rs:1184-1258` was built to
catch (`obj.contains_key("since") || obj.contains_key("type")` — i.e. a
node whose loaded "properties" look like a relationship's), but that code
only detects and logs the symptom; it cannot recover the lost data or
repair the misaligned scan.

### Trigger (confirmed by code inspection)

```
CREATE (a:Person {name:'Alice Alice Alice'})   -- long value, some node b created right after it on disk
SET a.name = 'Al'                              -- in-place shrink; stale tail bytes left on disk
-- restart / reopen the store --
CREATE (c:Person {name:'Carol'})               -- or any SET; allocates from the now-wrong next_offset
MATCH (b) WHERE id(b) = <b's id> RETURN b      -- b's properties are now garbage / overwritten by c's
```

## What Changes

- On an in-place shrink, zero the freed tail bytes
  (`offset+13+new_data_size` .. `offset+13+existing_data_size`) so no stale
  JSON fragment remains on disk to be mis-scanned.
- Persist the entry's PHYSICAL size independent of the payload `data_size`
  — either keep a separate "allocated size" field in the header that only
  ever grows or stays fixed (never shrinks on an in-place update), or
  always write a grow-only entry (never reuse a smaller footprint in
  place) so `data_size` and physical footprint never diverge.
- Change `rebuild_index` (`:539-599`) and `ensure_index_populated`
  (`:723-756`) to stride by that persisted PHYSICAL size, not by the
  payload `data_size`, so a shrunk entry's true on-disk footprint is always
  respected during recovery.

## Impact

- Affected specs: `docs/specs/storage-format.md` (property store entry
  layout — header must carry a stable physical-size field)
- Affected code: `crates/nexus-core/src/storage/property_store.rs`
  (`update_properties:280-334` in-place branch, `rebuild_index:539-599`,
  `ensure_index_populated:715-756`); `crates/nexus-core/src/storage/record_store_ops.rs:1184-1258`
  (existing corruption-detection fallback — stays as defense in depth, does
  not fix the root cause)
- Breaking change: on-disk format changes if a physical-size field is added
  to the header; needs a compatible migration/rebuild path for existing
  stores (consistent with the project's existing `rebuild_index` recovery
  mechanism)
- User benefit: `SET`/`REMOVE` that shortens a node's or relationship's
  properties can no longer corrupt an unrelated entity's data after restart
- Related: `phase0_fix-update-node-index-divergence`,
  `phase0_fix-delete-node-dangling-relationships` (sibling write-path/
  index-corruption defects from the same audit)
