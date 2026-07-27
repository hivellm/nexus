# Proposal: phase0_fix-optional-match-var-scoping

**Priority: HIGH — OPTIONAL MATCH returns wrong rows or silently drops rows that
Cypher requires to be preserved with NULL, whenever the OPTIONAL MATCH pattern's
first node is not the previously-bound anchor.** Found during a planner
correctness audit; not previously reported.

## Why

For every OPTIONAL MATCH, the planner builds `last_optional_vars` — the variable
set that the WHERE clause and the `OptionalFilter` operator use to decide which
variables may legitimately be NULL versus which are the mandatory grouping keys.
It computes this set by unconditionally skipping the pattern's FIRST node, on the
assumption that "the first node is the already-bound anchor"
(`crates/nexus-core/src/executor/planner/queries/planner_core.rs:333-334,340-342`):

```rust
// IMPORTANT: Skip the first node as it's typically the "anchor" that's already bound
// Only include variables from subsequent nodes and relationships
...
let mut is_first_node = true;
for element in &match_clause.pattern.elements {
    match element {
        PatternElement::Node(node) => {
            if is_first_node {
                // Skip the first node - it's the anchor from previous MATCH
                is_first_node = false;
            } else if let Some(var) = &node.variable {
                last_optional_vars.push(var.clone());
            }
        }
        ...
```

This assumption is false whenever (a) a standalone OPTIONAL MATCH's first node is
itself a fresh, genuinely optional variable, or (b) the pattern is written in
reverse direction so the already-bound anchor appears as a LATER node rather than
the first. `last_optional_vars` feeds `optional_vars` on the WHERE clause
(`strategy.rs:424-441`), which `execute_optional_filter`
(`crates/nexus-core/src/executor/operators/filter.rs:343-467`, semantics at
`:380-439`) uses to decide row preservation.

### Consequences (confirmed by code inspection)

- Reverse-direction pattern: `MATCH (a:Person) OPTIONAL MATCH
  (b:Person)-[:KNOWS]->(a) WHERE b.age > 30 RETURN a.name, b.name` — the
  genuinely optional first node `b` is skipped, and the already-bound anchor `a`
  is added to `last_optional_vars` instead. `optional_vars` becomes `[a]`, so
  `execute_optional_filter` treats the BOUND `a` as nullable and groups by `b` —
  inverted OPTIONAL semantics.
- Empty-`optional_vars` case: `MATCH (a:Person) OPTIONAL MATCH (c:Company) WHERE
  c.rating > 4 RETURN a.name, c.name` — the pattern's only node `c` is skipped as
  "the anchor", `last_optional_vars` stays empty, and `strategy.rs:426-428`
  therefore lowers the WHERE as a plain `Filter` instead of `OptionalFilter`.
  `Person` rows with no qualifying company are DROPPED instead of preserved with
  `c = NULL` — exactly the LEFT OUTER JOIN guarantee OPTIONAL MATCH exists to
  provide.

This is a silent correctness defect: no error, no warning — the query returns a
plausible-looking but wrong row set.

## What Changes

- Stop identifying the anchor by pattern POSITION. Compute `optional_vars` as the
  OPTIONAL MATCH pattern's variables MINUS the variables already bound by prior
  clauses (the running bound-variable set the planner tracks while walking
  `query.clauses`) — this is correct regardless of which position in the pattern
  the anchor occupies, and handles reverse-direction patterns and standalone
  OPTIONAL MATCH (no prior bound vars at all) uniformly.
- No operator-level format change: `Operator::OptionalFilter { predicate,
  optional_vars }` (`strategy.rs:436-439`) and `execute_optional_filter`
  (`operators/filter.rs:343-467`) keep their existing contract; only how
  `optional_vars` is computed changes.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (OPTIONAL MATCH / LEFT OUTER
  JOIN semantics)
- Affected code: `crates/nexus-core/src/executor/planner/queries/planner_core.rs`
  (`:331-370`, the `is_first_node` skip),
  `crates/nexus-core/src/executor/planner/queries/strategy.rs` (`:424-441`, WHERE
  lowering), `crates/nexus-core/src/executor/operators/filter.rs` (`:343-467`,
  `execute_optional_filter`)
- Breaking change: NO — corrects existing OPTIONAL MATCH semantics to match
  documented LEFT OUTER JOIN behavior; any application currently relying on the
  wrong rows was relying on a bug
- User benefit: OPTIONAL MATCH returns Neo4j-compatible LEFT OUTER JOIN results
  for standalone OPTIONAL MATCH and reverse-direction patterns; no more silently
  dropped or inverted rows
- Related: `phase0_fix-plan-reorder-drops-predicates`,
  `phase0_fix-where-predicate-reparse-precedence` — other WHERE/planner
  correctness defects found in the same audit
