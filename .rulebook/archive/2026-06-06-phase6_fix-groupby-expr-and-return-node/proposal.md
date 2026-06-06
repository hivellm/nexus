# Proposal: phase6_fix-groupby-expr-and-return-node

Source: GitHub issue #5 (https://github.com/hivellm/nexus/issues/5)

## Why
Two Cypher-execution quirks on `hivehub/nexus:2.2.0` force awkward query
rewrites downstream (Cortex coverage/inventory dashboards).

1. Implicit GROUP BY by a function/expression key drops the aggregation.
   Grouping by a bare property works, but grouping by an expression
   silently breaks:
   - `MATCH (n) RETURN labels(n)[0] AS label, count(n) AS c` returns one
     raw row per node with columns `["labels(n)"]` and no count.
   - Same via `WITH labels(n)[0] AS label, count(*) AS c` is also broken.
   Per openCypher, any non-aggregating projection term is an implicit
   grouping key — including expressions, not just bare properties.

2. `RETURN <nodeVar>` returns `null` instead of the node object.
   - `CREATE (t:ProbeNode {...}) RETURN t` -> `rows:[[null]]` though the
     node is created and its properties are readable when selected
     explicitly (`RETURN t.id, t.title` works).
   Expected: `RETURN t` serializes the node (id + labels + properties).
   Today callers must enumerate every property, coupling queries to schema.

## What Changes
- Implicit GROUP BY: treat every non-aggregating projection term as a
  grouping key, including computed expressions (`labels(n)[0]`,
  `head(labels(n))`, function calls, index access). Aggregates
  (`count`, `sum`, ...) group over the distinct key tuples. Output column
  names follow the projection aliases (`label`, `c`).
- Apply the same rule in both `RETURN ...` and `WITH ...` projections.
- `RETURN <nodeVar>`: serialize a bound node variable to the standard
  node object (internal id + labels + property map), matching the format
  used elsewhere. Same for relationship variables if affected.

## Impact
- Affected specs: cypher-subset / aggregation, cypher-subset / projection
- Affected code: `crates/nexus-core/src/executor/` (projection / grouping
  key derivation, aggregation operator, node/var serialization)
- Breaking change: NO (response row format unchanged; null -> correct value)
- User benefit: expression-keyed histograms work server-side; node
  variables return full objects without enumerating every property
