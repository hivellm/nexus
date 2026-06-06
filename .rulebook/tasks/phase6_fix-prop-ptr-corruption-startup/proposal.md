# Proposal: phase6_fix-prop-ptr-corruption-startup

Source: GitHub issue #4 (https://github.com/hivellm/nexus/issues/4)

## Why
`hivehub/nexus:2.2.0` emits a cascade of `[read_node] prop_ptr corruption
detected (points to Relationship <id>), resetting to 0` ERROR warnings on
every container boot. The same node ids recur across container recreates
on a persistent volume, meaning the corrective write is either not
persisted to the catalog before shutdown or the corruption is
re-introduced at write time. Observed downstream side effects:
- `MATCH (s) RETURN s LIMIT 1` intermittently returns
  `{"error":"JSON error: expected value at line 1 column 1"}` during the
  recovery window — a race between the corrupted read and the serializer.
- Affected nodes return only `_nexus_id` (lost property map) on
  `MATCH (n) WHERE id(n) = <nid> RETURN n`, even after the recovery logs.

A node `prop_ptr` pointing at a relationship id is a storage-integrity
violation; recovery via reverse_index masks but does not resolve it.

## What Changes
- Determine whether corruption originates at write time (wrong ptr type
  written) or during WAL replay (entries applied out of order).
- Make the recovery write durable: flush the reset `prop_ptr` (and the
  reverse_index-recovered property chain) to the catalog mdb / record
  store so subsequent boots are clean (one-shot recovery), OR fix the
  write/replay source so corruption never occurs.
- Eliminate the serializer race: a node whose `prop_ptr` is being
  recovered must not be serialized in a half-read state (no empty/invalid
  JSON; whole-node serialization must wait for or skip in-flight recovery).
- Ensure recovered nodes return their full property map on subsequent
  `RETURN n`, not just `_nexus_id`.

## Impact
- Affected specs: storage-format / wal-mvcc
- Affected code: `crates/nexus-core/src/storage/` (read_node, prop_ptr
  validation/recovery, durability of corrective writes); WAL replay path
- Breaking change: NO (data-integrity fix; on-disk format unchanged)
- User benefit: clean startup logs, no recurring corruption, no serializer
  race, reliable whole-node reads on a persistent volume
