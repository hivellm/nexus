# Legacy single-hop and variable-length relationship operators must share the same `Vec<u32>` `type_ids` shape

**Category**: correctness

**Tags**: cypher, executor, planner, relationship-traversal, variable-length-path, quantified-path-pattern

## Description

`Operator::Expand` (single-hop) always carried `type_ids: Vec<u32>` ("empty
= match all, populated = OR'd membership test"), but the sibling
`Operator::VariableLengthPath` (the legacy `*m..n` operator) carried
`type_id: Option<u32>` — a single value. The planner lowering built the full
`Vec<u32>` from the parsed relationship types (`[:R1|R2|R3]`) for BOTH
operators, but then narrowed it for `VariableLengthPath` via
`type_ids.first().copied()`, silently discarding every type but the first.
`[:KNOWS|FOLLOWS*1..3]` traversed only `:KNOWS`; rows reachable solely via
`:FOLLOWS` vanished with no error, no warning, no notification.

The bug was invisible to any test that only exercised a single relationship
type in a variable-length pattern — the `Option<u32>` slice-reconstruction
(`type_id.into_iter().collect()`) round-tripped perfectly for the `n == 1`
case, so single-type variable-length queries (`[:KNOWS*1..3]`) and
unqualified queries (`[*1..3]`, which lowers to an empty `Vec` = `None`)
both worked. Only `n >= 2` named types exposed the defect.

When two operators model the same relational concept (here: "traverse
these relationship types"), any type-shape divergence between them (`Vec`
vs `Option`) is a standing invitation for exactly this "only the field
that survives the narrowing conversion behaves correctly" class of bug.
The `QuantifiedExpand` operator (the newer QPP rewrite path, opt-in via
`NEXUS_QPP_REWRITE_LEGACY=1`) never had this problem because its
`QppHopSpec.type_ids` was already `Vec<u32>` from day one — it inherited
the correct shape by not going through a narrowing step at all.

## Example

```rust
// executor/types.rs — BEFORE (buggy divergence)
Operator::Expand { type_ids: Vec<u32>, /* ... */ }
Operator::VariableLengthPath { type_id: Option<u32>, /* ... */ }

// executor/planner/queries/relationships.rs — BEFORE (the actual bug)
let type_id = type_ids.first().copied(); // <-- drops every type but #0
operators.push(Operator::VariableLengthPath { type_id, /* ... */ });

// AFTER — mirror the sibling operator's shape exactly, no narrowing step
Operator::VariableLengthPath { type_ids: Vec<u32>, /* ... */ }
operators.push(Operator::VariableLengthPath {
    type_ids: type_ids.clone(),
    /* ... */
});

// operators/path.rs — thread the slice straight through, matching
// find_relationships' existing "empty = match all" contract instead of
// reconstructing a one-element Vec from an Option every BFS iteration:
pub(in crate::executor) fn execute_variable_length_path(
    &self,
    context: &mut ExecutionContext,
    type_ids: &[u32],   // was: type_id: Option<u32>
    /* ... */
) -> Result<()> {
    // ...
    let neighbors = self.find_relationships(current_node, type_ids, direction, None)?;
```

## When to Use

Whenever two operators (or two functions) in the same execution layer
model the same underlying concept with different representations for the
same field — one `Vec<T>`, the other `Option<T>` or a single `T` — treat
that divergence itself as a defect to investigate, even if all existing
tests pass. Grep for the narrower type's every construction site and every
consumer; confirm the narrowing point (`.first()`, `.pop()`,
`unwrap_or_default()`, etc.) isn't silently dropping data the wider
sibling would have kept. When fixing, retype to match the sibling exactly
(same "empty = match all" convention) rather than inventing a new
representation — this keeps the two operators' semantics provably
identical and lets tests assert parity between them directly (e.g. running
the same query through both the legacy operator and its rewrite-path
sibling and diffing the rows).

## When NOT to Use

Don't force a `Vec<T>` shape onto genuinely single-valued fields just
because a "nearby" operator happens to use a collection — audit the
Cypher grammar first. Some relationship-traversal call sites
(`shortestPath()` / `allShortestPaths()` in `fn_graph.rs`, for example)
independently derive their own `type_id: Option<u32>` from
`rel.types.first().cloned()` and are a *pre-existing, separate*
single-type limitation, not a shape mismatch against a `Vec`-typed
sibling in the same layer — don't conflate the two or widen scope beyond
the operator that actually has a sibling to mirror.
