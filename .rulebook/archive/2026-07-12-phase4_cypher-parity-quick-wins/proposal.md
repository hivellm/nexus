# Proposal: phase4_cypher-parity-quick-wins

## Why

The compatibility gap analysis
([docs/nexus/01-compatibility-gaps.md](../../../docs/nexus/01-compatibility-gaps.md))
shows Nexus at ~85% openCypher/Neo4j parity with a tail of small, cheap
missing functions and format mismatches that generate diff-suite asterisks
and SDK friction far out of proportion to their implementation cost. All are
S-effort executor-dispatch additions; none touch storage or planning.

## What Changes

Implement the missing scalar functions and format fixes:

- `randomUUID()` — v4 UUID string
- String: `ascii()`, `chr()`, `lpad()`, `rpad()`, `normalize()` (NFC default,
  form argument optional)
- Math: `log(x, base)` two-arg form; `isNaN()`
- List: `shuffle()` (note: use a seedable RNG; document non-determinism)
- `elementId()` — emit Neo4j-5-style opaque stable string instead of raw
  internal 64-bit ID (keep `id()` returning the integer)
- Verify + fix if broken: `percentileDisc`, `percentileCont`, `stDev`,
  `stDevP` (declared in cypher-subset.md, implementation unverified)
- Multiple patterns in one CREATE: `CREATE (a:L), (b:L)`

## Impact

- Affected specs: specs/functions/spec.md (this task)
- Affected code: `crates/nexus-core/src/executor/eval/projection/` (function
  dispatch), parser for multi-pattern CREATE
- Breaking change: `elementId()` output format changes (was internal ID) —
  documented in CHANGELOG; `id()` unchanged
- User benefit: fewer "works in Neo4j, fails in Nexus" paper cuts; diff-suite
  coverage extends.
