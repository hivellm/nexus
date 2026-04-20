# Quantified Path Patterns — Technical Design

## Background

Cypher 9's variable-length paths (`-[:TYPE*1..5]->`) quantify a single
relationship. Cypher 25 (aligned with ISO GQL) introduces
**Quantified Path Patterns (QPP)**: a parenthesised path fragment
followed by a quantifier that applies to the whole fragment.

```cypher
-- Legacy (Nexus today): quantify a single relationship
MATCH (a)-[:REPORTS_TO*1..5]->(b) RETURN a, b

-- QPP: quantify a whole parenthesised path
MATCH (a) ( (x)-[:REPORTS_TO]->(m {role: 'manager'}) ){1,5} (b)
RETURN a, m, b  -- m is now LIST<NODE>
```

QPP is required because variable-length paths cannot express
intermediate-node filters, inter-hop predicates, or path-variable
collection.

## Grammar

### Quantifier tokens

```
quantifier := '{' int (',' int?)? '}'
            | '{' ',' int '}'
            | '+' | '*' | '?'
```

Desugaring:

- `+`    → `{1,}`
- `*`    → `{0,}`
- `?`    → `{0,1}`
- `{n}`  → `{n,n}`

### Pattern extension

```
pattern_part    := element_pattern (pattern_part)*
element_pattern := node_pattern
                 | rel_pattern
                 | quantified_path
quantified_path := '(' path_fragment ')' quantifier
path_fragment   := node_pattern rel_pattern node_pattern (rel_pattern node_pattern)*
```

Nesting is allowed **one level deep** in v1 (GQL spec permits deeper;
deferred to v2 to keep the planner tractable).

### Disambiguation

The parser must distinguish `{a: 1}` (map literal) from `{1,5}`
(quantifier). Lookahead: after `}`, if the previous `{...}` contents
match `<int>(,<int>?)?` or `,<int>`, treat as a quantifier. Otherwise
re-enter as a map literal. Both are unambiguous because a quantifier
can only appear after `)` in a pattern.

## AST

```rust
pub enum PatternPart {
    Node(NodePattern),
    Relationship(RelPattern),
    Quantified {
        fragment: Box<PathFragment>,
        quantifier: Quantifier,
    },
}

pub struct Quantifier {
    pub lower: u32,           // 0..inclusive
    pub upper: Option<u32>,   // None = unbounded
}

pub struct PathFragment {
    pub parts: Vec<PatternPart>,   // one level of nesting allowed
    pub inner_bindings: Vec<Binding>,
}
```

### Type promotion rule

Every variable declared inside a quantified fragment is promoted to
`LIST<T>` in the enclosing scope. Indexing `list[i]` yields the
binding at iteration `i`. This matches the GQL spec and Neo4j 5.9+.

```cypher
MATCH ( (x)-[r:R]->(y) ){1,3}
RETURN x, r, y, length(x)
-- x :: LIST<NODE>, r :: LIST<RELATIONSHIP>, y :: LIST<NODE>
-- length(x) returns 1, 2, or 3 depending on the matched iteration count
```

## Planner: QuantifiedExpand operator

```rust
struct QuantifiedExpand {
    pub source: BindingSet,            // rows entering the QPP
    pub fragment: CompiledFragment,    // compiled inner plan
    pub lower: u32,
    pub upper: Option<u32>,
    pub cycle_policy: CyclePolicy,
    pub projected: Vec<VarSlot>,       // LIST slots to fill
}
```

### Execution

1. For each input row, push a **frame** with the starting binding.
2. If current iteration count `k` satisfies `k >= lower`, emit the
   current accumulated bindings as a valid match.
3. If `k < upper` (or `upper` is unbounded), run the fragment once
   more with the last node as the new starting point, producing
   candidates.
4. For each candidate, push a child frame and recurse.
5. On exhaustion, pop back to previous frame and try alternatives.

### Cycle policy

- Inside `MATCH`: **NODES_CAN_REPEAT** (default, matches Neo4j).
- Inside `shortestPath`/`allShortestPaths`: **ALL_DIFFERENT** nodes.

Both policies are already implemented for variable-length paths;
QPP reuses the same cycle-detection bitset per frame.

### Memory budget

Each frame costs `O(|inner_bindings|)`. Recursion depth is bounded by
`upper` (or by the per-query safety cap `max_qpp_depth = 64` when
unbounded). Worst-case memory = `O(upper * |inner_bindings|)`.

## Cost model

Given an average inner-fragment fanout `f`:

- Unquantified single hop cost = `f`
- `{m,n}` cost ≈ `Σ_{k=m..n} f^k` (geometric series)

The planner caps `upper` at the per-query safety limit during cost
estimation so `unbounded` does not collapse the cost model. When the
user writes `*` without a predicate that could terminate the
traversal (e.g. a terminal node label), the planner emits a warning
but still runs (consistent with Neo4j's behaviour).

## Pushdown of inner predicates

Predicates on iteration-local variables are pushed into the inner
plan:

```cypher
MATCH ( (x)-[:R]->(y) WHERE y.active ){1,5}
```

→ the inner plan's Expand operator receives `y.active` as a
post-filter, not the outer plan. This mirrors Neo4j 5.x's planner.

## Rewriter for legacy variable-length paths

To keep the executor simple, legacy `-[:T*m..n]->` is rewritten at
parse time into QPP form:

```
-[r:T*1..5]->(b)
   becomes
( (a_tmp)-[r_inner:T]->(a_tmp_next) ){1,5} (b)
```

with the special binding `r = r_inner`. This means there is a single
code path for quantification; no duplicate operators. Regression tests
assert the plans for `*m..n` queries before and after the rewrite are
executionally identical (different AST, same row output).

## shortestPath / allShortestPaths

Neo4j 5.9+ allows `shortestPath(qpp)` where the argument is a
quantified path. Our implementation:

- BFS from the start node; each BFS level corresponds to one QPP
  iteration.
- Frontier = set of bindings after k iterations that satisfy the
  inner fragment.
- Terminate at the first level where the terminal binding is reached.

`allShortestPaths` continues BFS one more level and collects all
frontier states at the minimum-length level.

## Error taxonomy

| Code                        | Condition                                       |
|-----------------------------|-------------------------------------------------|
| `ERR_QPP_INVALID_QUANTIFIER`| `{n,m}` with `n > m`, or negative ints          |
| `ERR_QPP_NESTING_TOO_DEEP`  | More than 1 level of nested QPP (v1 limit)      |
| `ERR_QPP_UNBOUND_UPPER`     | `*` used in a context where safety cap exceeded |
| `ERR_QPP_EMPTY_FRAGMENT`    | `()` quantified (empty path fragment)           |

All are parse-time errors surfaced as HTTP 400 with positional span.

## TCK coverage

openCypher TCK `features/path-pattern/quantified-*.feature` has ~140
scenarios. Target: 95% pass rate in v1, 100% in v1.1 (after nested
QPP v2 work lands).

## Performance targets

| Scenario                              | Target p95 |
|---------------------------------------|------------|
| `{1,3}` over 10k-node graph           | < 5 ms     |
| `{1,5}` friend-of-friend, 100k nodes  | < 80 ms    |
| `shortestPath(qpp)`, diameter 6 graph | < 50 ms    |
| Regression vs legacy `*m..n`          | ≤ 1.3×     |

## Out of scope for v1

- Nesting depth > 1 (tracked v1.1).
- Quantifiers on single relationships using `{m,n}` (legacy `*m..n`
  stays the user-facing spelling; normalised internally).
- Path variables bound at the QPP level (`MATCH p = (...){1,5}`) —
  tracked v1.1 after reviewing GQL editor-draft semantics.

## Rollout

- v1.2.0 ships QPP behind feature flag `cypher_qpp_enabled` default
  **on**. Flag exists only as an emergency disable during the first
  release cycle; removed at v1.3.
- Neo4j diff harness extended with ~80 QPP scenarios drawn from the
  official TCK subset.
