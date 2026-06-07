# Proposal: phase6_clean-graph-rebuild-null-ids

Source: field report (Cortex 2.3.0 validation) — "phase25 §4: rebuild limpo
do graph (null ids legados)". Companion to #7 (`parameters: null`) and #8
(read-side index seek / comma-join cartesian).

## Why
Legacy nodes ingested before the 2.3.x fixes carry `null` `id` / `name`
properties. Those rows can't be addressed by an index seek (`MATCH (n:Label
{id: $v})` never matches a null-keyed node) and pollute label scans, so the
graph needs a clean rebuild once the read-side index seek (#8) lands. This
task covers the Nexus-side support for that rebuild; the actual re-ingest is
driven by the downstream client (Cortex bootstrap).

## What Changes
- Determine the Nexus-side need (investigation first):
  - Does MERGE/index handling behave correctly when a property value is JSON
    `null` (e.g. `MERGE (n:L {id: null})`) — does it create a phantom
    null-keyed node, or should null keys be rejected / skipped from the
    property index? Define and enforce the contract.
  - Provide a clean way to wipe + rebuild a graph/database so the client can
    re-bootstrap deterministically (confirm `DROP DATABASE` / a bulk clear +
    fresh ingest path works and indexes/adjacency rebuild correctly).
- Document the rebuild procedure (drop → recreate indexes → re-ingest) so the
  downstream worker can run it once the index-seek + parameters fixes ship.

## Impact
- Affected specs: storage / catalog, cypher-subset / merge, ops / rebuild
- Affected code: `crates/nexus-core/src/engine/` (null-property handling in
  MERGE/index), database drop/clear path; docs under `docs/`
- Breaking change: NO
- User benefit: legacy null-keyed data no longer blocks index seeks; a
  documented, deterministic clean-rebuild path unblocks the graph worker.

## Notes
- Sequencing: ships after #8 (read-side index seek); the client re-ingest is
  the consumer. Confirm whether any Nexus code change is actually required or
  whether §4 is purely a downstream re-bootstrap (close as docs-only if so).
