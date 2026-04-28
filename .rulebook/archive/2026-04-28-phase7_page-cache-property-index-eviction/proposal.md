# Proposal: phase7_page-cache-property-index-eviction

## Why

`crates/nexus-core/src/cache/mod.rs` has a `// TODO: Check if index is actually cached` marker on the property-index lookup path. The eviction policy for property-index pages is not implemented — they live in the page cache but no LRU/TTL/Clock policy currently bounds their footprint. Under workloads with high-cardinality property indexes (e.g. unique constraint on a UUID property over 10 M+ nodes) memory grows unpredictably. The hot-path data-page eviction is solid (Clock / 2Q / TinyLFU); only the property-index slice is unfinished. This is a medium-severity correctness-of-behaviour issue (the engine works, but resource use under load is undefined) and a Tier-1 violation (TODO in shipping code).

## What Changes

- Remove the `// TODO: Check if index is actually cached` comment by implementing the missing branch.
- Add an LRU policy on property-index entries (separate slice from the data-page Clock cache, sized via env var with a sensible default — start at e.g. 64 MB).
- Track cache hit / miss / eviction stats for the property-index slice and surface them on `/stats`.
- Add an integration test that creates many property indexes, fills them past the cap, and asserts stable RSS within a tolerance.

## Impact

- Affected specs: `docs/specs/page-cache.md` (extend with property-index slice section).
- Affected code: `crates/nexus-core/src/cache/mod.rs`, possibly `crates/nexus-core/src/index/property_index.rs`, `/stats` endpoint.
- Breaking change: NO (only adds a bound; default may evict more aggressively under high pressure).
- User benefit: predictable memory footprint under high-cardinality workloads.
