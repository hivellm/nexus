# Proposal: phase21_tck-null-property-write-semantics

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Surfaced during an independent review of the TCK conformance commits.

## Why
openCypher treats a map key written with a null value as **absent**: `CREATE (n
{id: 12, name: null})` reports `+properties 1` and the node has no `name` key.
Today only the *counter* implements that rule, not the write itself.

`crates/nexus-core/src/storage/record_store_ops/create.rs:130-133` filters null
values out of `inline_prop_count`, but the **unfiltered** `properties` map is
what reaches `store_properties_if_any` a few lines later (`:158`, `:236-240`),
and `store_properties` serialises it verbatim. `is_null()` appears exactly twice
in the whole storage write path — `create.rs:132` and `create.rs:319` — both
times inside a side-effect counter. Nothing filters nulls before the bytes land.

The graph state therefore disagrees with the counter that reports it:

```
CREATE (n {id: 12, name: null}) RETURN keys(n)
  → ["id", "name"]      (openCypher/Neo4j: ["id"])
  but +properties = 1   ← the value the TCK asserts
```

`keys()` (`executor/eval/projection/fn_graph.rs:184-197`) excludes only
`_`-prefixed keys and `type`, never nulls, so the phantom key is observable. The
regression test added alongside the counter change
(`tests/cypher/side_effect_counting_test.rs`) asserts only the counter — never
`keys(n)`, `properties(n)`, or `n.name IS NULL` — which is exactly what would
have exposed the divergence.

The same rule governs the update paths: writing null to an existing property
must **remove** it (`-properties`), and writing null to an absent one must be a
no-op. Those paths need auditing too, not just CREATE.

## What Changes
Filter null-valued keys out of the property map **itself**, before it reaches
`store_properties` / `store_properties_if_any`, and let the side-effect counter
derive from the filtered map instead of carrying its own private filter — one
rule, applied once, at the point where it is observable.

Audit every inline-property write path for the same semantics: CREATE, MERGE
(`ON CREATE` / `ON MATCH`), `SET n.p = null`, `SET n = {map}` (whole-entity
replace), and `SET n += {map}`. For the update paths, writing null to an
existing key must remove it and count `-properties`.

## Impact
- Affected specs: docs/specs/cypher-subset.md (property write semantics — null
  means absent)
- Affected code: crates/nexus-core/src/storage/record_store_ops/create.rs,
  crates/nexus-core/src/storage/property_store/crud.rs,
  crates/nexus-core/src/engine/write_exec/properties.rs,
  crates/nexus-core/src/executor/operators/create.rs
- Breaking change: NO on the wire (current behavior stores a key the spec calls
  absent, so removing it moves toward the spec). Nodes already written by the
  current code may carry null-valued keys on disk; reads must stay tolerant of
  them rather than assuming the invariant holds retroactively.
- User benefit: `keys()`, `properties()`, and property access agree with the
  `+properties` counter and with Neo4j. Closes a class of `clauses/create`,
  `clauses/set` and `clauses/merge` scenarios where the counter is right but the
  graph state is wrong — and removes a divergence no existing test could catch.
