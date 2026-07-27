# OPTIONAL MATCH nullable-variable set is a set difference against bound vars, not a positional skip

**Category**: code
**Tags**: cypher, optional-match, planner, left-outer-join, variable-scoping

## Description

When a planner walks an OPTIONAL MATCH pattern to decide which variables are "new" (and therefore must be nulled out by the LEFT OUTER JOIN fallback when nothing matches), never assume the already-bound anchor sits at a fixed textual position (e.g. "the first node"). That breaks for reverse-direction patterns (`OPTIONAL MATCH (b)-[:KNOWS]->(a)` where `a`, not `b`, is the bound anchor) and for standalone patterns with no anchor at all (`OPTIONAL MATCH (c:Company)` — skipping "the first node" positionally leaves the nullable set empty, so a subsequent WHERE gets lowered as a plain `Filter` instead of an `OptionalFilter`, silently dropping driver rows instead of preserving them with `c = NULL`).

Compute the nullable set as `(all variables in the pattern) - (variables already bound by prior clauses)` instead. This requires a `HashSet<String>` accumulator threaded through the clause-walking loop that:
1. Is populated by every regular MATCH clause's pattern variables.
2. Is ALSO populated by every OPTIONAL MATCH clause's own pattern variables, immediately after computing that clause's (possibly non-empty) nullable set — because those variables are still in lexical scope for a *later* clause, even though their runtime value may be NULL. Skipping this step breaks chained `OPTIONAL MATCH ... OPTIONAL MATCH` queries: the second clause's diff would wrongly re-include the first clause's already-resolved variable as "new", corrupting the `OptionalFilter` mandatory/optional grouping split and spuriously nulling out a variable that legitimately matched.

Check whether a bound-variable accumulator already exists in scope at the point the pattern is walked before introducing a new one — in this codebase a similarly-named `previously_bound_vars: HashSet<String>` already existed, but in a different function (`strategy.rs`, built later from the already-resolved pattern list, for a completely different purpose: deciding which `NodeByLabel` operators to skip). It was out of scope at the clause-walking site and could not be reused; a new accumulator had to be introduced in the earlier function that walks `query.clauses` directly.

## Example

fn collect_pattern_variables(pattern: &Pattern) -> Vec<String> { /* node + rel + quantified-group vars, no positional skip */ }

let mut bound_vars: HashSet<String> = HashSet::new();
for clause in &query.clauses {
    if let Clause::Match(m) = clause {
        let pattern_vars = collect_pattern_variables(&m.pattern);
        if m.optional {
            let nullable: Vec<String> = pattern_vars.iter()
                .filter(|v| !bound_vars.contains(*v)).cloned().collect();
            // nullable -> Operator::OptionalFilter { optional_vars: nullable, .. }
        }
        bound_vars.extend(pattern_vars); // in scope for every clause after this one
    }
}

## When to Use

Any planner/compiler pass that needs to classify a pattern's variables as "new" vs "already bound" to drive LEFT OUTER JOIN / OPTIONAL nullability semantics — not just Cypher OPTIONAL MATCH.

## When NOT to Use

Don't reach for this when the anchor position genuinely IS guaranteed by grammar (no such guarantee exists in openCypher's OPTIONAL MATCH — direction and anchor placement are caller-controlled).
