# Proposal: phase6_fix-rest-parameters-null

Source: GitHub issue #7 (https://github.com/hivellm/nexus/issues/7)

## Why
`POST /cypher` on 2.3.0 returns **HTTP 422** when the body carries
`"parameters": null`. The field accepts a map (`{}`) or omission, but not
explicit `null`. The published `nexus-graph-sdk` 2.1.0 serializes
`parameters` as explicit `null` for no-param queries, so EVERY no-parameter
Cypher call from an SDK client 422s against a 2.3.0 server. This is a
regression — 2.2.0 accepted `parameters: null`. The 2.3.0 param work added
`#[serde(default, alias = "parameters")]`, which handles absent/`{}` and the
`parameters` key, but serde still rejects an explicit `null` for a
non-`Option` `HashMap`.

## What Changes
- Make the REST `CypherRequest.params` field treat explicit JSON `null` (and
  absent) as an empty map, restoring 2.2.0 behaviour — e.g. a
  `deserialize_with` helper that maps `null`/missing → `HashMap::new()`,
  keeping the `parameters` alias and the `params` name both working.
- Confirm `query: "...", parameters: null` → 200 with an empty param map;
  `{}` and omitted still 200; a real map still binds.

## Impact
- Affected specs: api-protocols (cypher request body)
- Affected code: `crates/nexus-server/src/api/cypher/mod.rs` (`CypherRequest`)
- Breaking change: NO (restores accepted input; no response-format change)
- User benefit: published SDK 2.1.0 clients (and any client emitting
  `parameters: null`) work against 2.3.x without patching every call site
