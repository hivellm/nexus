# Dynamic-label sentinel in MATCH defers to a Filter, not a plan-time catalog lookup

**Category**: architecture
**Tags**: cypher, planner, dynamic-labels, executor, filter, silent-wrong-results

## Description

`MATCH (n:$label)` stores `$label` verbatim as a sentinel string in the node
pattern's `labels` list. The planner (`QueryPlanner`) has no access to query
parameters — those live only on the execution context, which is constructed
after planning. Calling `self.catalog.get_or_create_label("$label")` at plan
time therefore resolves (and even *creates*) a literal label named `$label`
instead of the runtime value, so the scan silently returns zero rows (a
"silent wrong results" bug, not a crash — the query parses and runs fine,
it just answers the wrong question).

The correct fix mirrors how `WHERE n:$x` already worked: defer the whole
resolution to execution. When the FIRST label of a node pattern starts with
`$`, the planner must NOT call `get_or_create_label` on it. Instead emit
`AllNodesScan` + `Filter("{variable}:{first_label}")` — literally the same
lowering the planner already uses for the *second and later* labels in a
multi-label pattern (`MATCH (n:A:B)` lowers `:B` to a Filter). The Filter
operator (`executor/operators/filter.rs`) already has a `$`-prefix branch
that resolves against `context.params` at execution time: a non-empty
STRING becomes the label, anything else (missing / NULL / empty /
non-STRING) collapses the predicate to "no rows" — never an error, mirroring
openCypher three-valued logic for labels.

## Example

```rust
// crates/nexus-core/src/executor/planner/queries/strategy.rs
if !node.labels.is_empty() {
    let first_label = &node.labels[0];
    if first_label.starts_with('$') {
        // Cannot resolve — planner has no params. Defer to the Filter
        // operator's existing `$`-prefix runtime resolution.
        operators.push(Operator::AllNodesScan { variable: variable.clone() });
        operators.push(Operator::Filter {
            predicate: format!("{}:{}", variable, first_label),
            predicate_ast: None,
        });
    } else {
        let label_id = self.catalog.get_or_create_label(first_label)?;
        // ...existing seek/scan emission using label_id...
    }
    // Additional labels (node.labels[1..]) already lower to Filter and
    // are untouched by this branch — they compose correctly either way.
}
```

## When to Use

Any time a planner needs to resolve a `$param`-sentinel value that is only
available on the execution context (labels, relationship types, property
keys referenced dynamically) and the executor already has a working runtime
resolution path for the same sentinel in a different clause (here, `WHERE`).
Reuse that existing runtime resolution by routing through the same operator
instead of inventing a second resolution mechanism at plan time.

## When NOT to Use

Do not resolve a `$`-prefixed sentinel via any catalog "get or create"
call at plan time — `get_or_create_label`/`get_or_create_type`/etc. treat
the literal sentinel string as a real name and will pollute the catalog
with a spurious literal label/type that generally has no members, silently
producing empty results instead of an error. If no downstream operator
already has runtime `$param` resolution for the entity kind in question,
that resolution needs to be built first — don't half-apply this pattern by
routing to a Filter that doesn't understand the sentinel.
