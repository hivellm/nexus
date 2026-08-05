# Proposal: phase22_diff-merge-multivar-return-and-with

> Neo4j differential suite, not the openCypher TCK. These are the only two
> failures in `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`
> (308/325, 15 skipped), measured twice against a live Neo4j 2025.09.0.
> Long-recorded as "pre-existing executor limits" and never opened as work.

## Why

Two shapes of `MERGE` fail on the differential suite, both because the ENGINE
write path — which intercepts `MERGE`/`SET`/`DELETE` before the executor sees them
— models bindings as `variable -> Vec<node_id>` and cannot express what these
queries ask for.

### 15.08 — a RETURN mentioning two different variables

```cypher
MERGE (a:Product {name: 'A'}) MERGE (b:Product {name: 'B'})
RETURN a.name AS a, b.name AS b
```
→ `Multiple different variables in RETURN not supported for write queries`
(`engine/write_exec/return_builder.rs:200-205`). Neo4j returns one row, `A | B`.

`build_return_result` iterates ONE variable's id list, so it rejects a second.
But an escape hatch for this already exists a few lines up: any RETURN carrying a
"complex expression" is delegated to the real executor by
`build_return_result_with_executor`, which materialises the ids into
`MATCH (v) WHERE id(v) IN [...] RETURN ...` and lets the executor apply its own row
semantics. That helper is merely hard-wired to a single variable — and it picks it
with `context.keys().next()`, an arbitrary choice out of a `HashMap`, which is a
latent bug of its own whenever the context holds more than one variable.

Reconstructing multi-variable rows inside the write path would be a guess: its
per-variable lists are INDEPENDENT, not row-aligned columns (recorded in
`project-multi-pattern-write-materialization`, and the reason `MERGE` of a
relationship takes the cartesian while `CREATE` zips). Delegating instead of
guessing keeps one row model rather than inventing a second.

### 15.12 — a WITH between two writes

```cypher
MERGE (n:Product {name: 'Unique'}) WITH n
MERGE (n2:Product {name: 'Unique'}) RETURN count(DISTINCT n) AS cnt
```
→ `Unsupported clause in write query` (`engine/write_exec/dispatch.rs:362-370`,
where `With` sits in a catch-all with `Unwind`/`Union`/`OrderBy`/`Limit`/`Skip`).
Neo4j returns `cnt = 1` — the second `MERGE` must MATCH the node the first created,
not duplicate it.

The same rejection was hit independently earlier from a different direction
(`MATCH … WITH n, duration(…) AS d SET n.d = d`), so this is not a MERGE-specific
gap; it is the write dispatcher having no notion of a scope boundary at all.

For the `variable -> ids` model a bare `WITH n` is exactly a scope cut: keep the
listed variables, drop the rest, optionally under a new name. That much is
directly representable. A projected property, expression or aggregation is NOT
(there is no id list to carry), so those must keep erroring rather than silently
dropping the projection.

## What Changes

- Generalise `build_return_result_with_executor` to every context variable the
  RETURN actually references — one `(v)` pattern and one `id(v) IN [...]` conjunct
  per variable — replacing the arbitrary `keys().next()` pick.
- `build_return_result` delegates to it when the RETURN spans more than one
  variable, instead of erroring.
- Implement `Clause::With` in the write dispatcher for **bare variable**
  projections (with optional alias): restrict the context to those variables. Any
  other projected item keeps the existing error, with a message that says what is
  and is not supported instead of a blanket "unsupported clause".

## Impact

- Affected specs: docs/specs/cypher-subset.md (write-path RETURN and WITH support)
- Affected code: crates/nexus-core/src/engine/write_exec/return_builder.rs,
  crates/nexus-core/src/engine/write_exec/dispatch.rs
- Breaking change: NO — both shapes error today, so nothing that works can break.
  Queries that currently fail will start returning rows.
- User benefit: the differential suite reaches 310/325; consecutive `MERGE`s can be
  returned together, and a `WITH` scope cut no longer makes a whole write query
  unexecutable.
