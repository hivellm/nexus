# Proposal: phase21_tck-comma-pattern-binding-materialization

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Surfaced during the WITH...WHERE/expand review (independent reviewer probe).

## Why
With a comma-separated multi-pattern MATCH, later pattern variables are never
materialized into the driving rows — they live only in `context.variables`.
A subsequent OPTIONAL MATCH that closes over such a variable sees
`row.get(target_var) == None`, treats the target as unbound, accepts any
candidate relationship, and silently REBINDS the variable. Confirmed probe:
graph `(a:A{n:'a'}), (z:Z{n:'z'}), (b:B)` with `(a)-[:T]->(b)`:
`MATCH (a:A), (z:Z) OPTIONAL MATCH (a)-[r:T]->(z) RETURN a.n, z.n, r IS NULL`
returns `["a","b",false]` — `z` is now the `:B` node. openCypher requires the
cartesian rows to keep `z` bound and pad `r` with NULL when no edge matches.
This caps clauses/with-where WithWhere1[3]/[4] and any scenario mixing comma
patterns with OPTIONAL MATCH closure.

## What Changes
Materialize every comma-pattern variable into the executor's driving rows
(cartesian product across the comma patterns) before downstream operators run,
so Expand sees pre-bound targets and its LEFT-OUTER padding applies. Audit
`materialize_rows_from_variables` and the planner's multi-pattern lowering for
where the row-binding is dropped; ensure the fix composes with the committed
expand padding guard (`!new_row.contains_key(target_var)`).

## Impact
- Affected specs: docs/specs/cypher-subset.md (MATCH cartesian semantics)
- Affected code: crates/nexus-core/src/executor/planner/queries/ (multi-pattern
  lowering), executor/operators/expand.rs interaction, eval/helpers/core.rs
  row materialization
- Breaking change: NO (current behavior silently rebinds variables — spec-wrong)
- User benefit: comma patterns + OPTIONAL MATCH produce correct rows; unblocks
  WithWhere1[3]/[4] and related TCK scenarios.
