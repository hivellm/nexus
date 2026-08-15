# Cypher Subset Specification

This document defines the Cypher query language subset supported by Nexus (MVP and V1).

## Design Philosophy

**20% syntax covering 80% use cases**:
- Focus on read-heavy workloads (MATCH/RETURN)
- Pattern matching with property filters
- KNN-seeded traversal (native procedure)
- Simple aggregations
- No complex subqueries initially

## Supported Syntax (MVP)

### MATCH Clause

```cypher
-- Single node
MATCH (n:Label)

-- Node with multiple labels
MATCH (n:Person:Employee)

-- Relationship pattern
MATCH (n:Person)-[r:KNOWS]->(m:Person)

-- Undirected relationship
MATCH (n:Person)-[r:KNOWS]-(m:Person)

-- Variable-length path ✅ IMPLEMENTED (with bounded-depth protection)
MATCH (n:Person)-[:KNOWS*1..3]->(m:Person)
MATCH (n:Person)-[:KNOWS*5]->(m:Person)  -- Fixed length
MATCH (n:Person)-[:KNOWS*]->(m:Person)   -- Unbounded (max 64 hops)
MATCH (n:Person)-[:KNOWS+]->(m:Person)   -- One or more (max 64 hops)
MATCH (n:Person)-[:KNOWS?]->(m:Person)   -- Zero or one

**Variable-length path bounded depth:** Unbounded quantifiers (`[*]`, `[+]`) and quantified path patterns with large bounds are clamped to a maximum **64-hop BFS depth**, preventing exhaustion on dense or cyclic graphs. Bounded quantifiers (e.g. `[*1..5]`) operate normally within their specified range. The limit applies per traversal start point; see `docs/specs/cypher-subset.md` § "Variable-Length Paths" for traversal semantics.

-- Quantified Path Patterns (Cypher 25 / GQL) ✅ FULLY IMPLEMENTED
-- Anonymous-body shape collapses to legacy *m..n at parse time;
-- named / labelled / multi-hop bodies route through the dedicated
-- QuantifiedExpand operator.
MATCH (a)( ()-[:KNOWS]->() ){1,5}(b)
MATCH (a)( ()<-[:OWNS]-() ){1,3}(b)
MATCH (a)( ()-[r:KNOWS]->() ){2,4}(b) RETURN length(r)
MATCH (a)( (m:Person)-[:KNOWS]->(n:Person) ){1,3}(b) RETURN m, n

-- Path-mode keywords (phase8_quantified-path-patterns-execution):
-- WALK | TRAIL | ACYCLIC | SIMPLE precede the QPP group and
-- constrain repeated edges / nodes across the matched path.
MATCH (a)WALK    ( ()-[:KNOWS]->() ){1,5}(b)   -- revisits OK (default)
MATCH (a)TRAIL   ( ()-[:KNOWS]->() ){1,5}(b)   -- no edge revisits
MATCH (a)ACYCLIC ( ()-[:KNOWS]->() ){1,5}(b)   -- no node revisits
MATCH (a)SIMPLE  ( ()-[:KNOWS]->() ){1,5}(b)   -- no edge AND no node revisits
-- See docs/guides/QUANTIFIED_PATH_PATTERNS.md for the full surface.

-- Multiple patterns
MATCH (a:Person)-[:KNOWS]->(b:Person)-[:WORKS_AT]->(c:Company)

-- Labelled scan returns only that label
MATCH (n:Person) RETURN n
-- Returns only nodes with the Person label, not unlabelled or other-labelled nodes

-- Unlabelled scan returns all nodes
MATCH (n) RETURN n
-- Returns every node in the database regardless of labels
```

**Pattern Syntax**:
```
Pattern ::= Node | Relationship | QuantifiedGroup | Pattern ',' Pattern

Node ::= '(' Variable? Labels? Properties? ')'

Labels ::= SP? ':' SP? Identifier ( SP? ':' SP? Identifier )*

Relationship ::= 
    '-[' Variable? ':' SP? Type Properties? Quantifier? ']->' |  // directed out
    '<-[' Variable? ':' SP? Type Properties? Quantifier? ']-'  |  // directed in
    '-[' Variable? ':' SP? Type Properties? Quantifier? ']-'      // undirected

QuantifiedGroup ::= PathMode? '(' Pattern ')' Quantifier
    -- Cypher 25 / GQL. Anonymous-body shapes still collapse to
    -- the legacy *m..n quantifier at parse time when no path
    -- mode keyword precedes them; explicit mode keywords (and
    -- any named / labelled / multi-hop body) route through the
    -- dedicated QuantifiedExpand operator with per-frame
    -- visited-set tracking.

PathMode ::= 'WALK'    -- revisits OK (implicit default)
           | 'TRAIL'   -- no edge revisits
           | 'ACYCLIC' -- no node revisits
           | 'SIMPLE'  -- no edge AND no node revisits

Quantifier ::= '*' Int? ('..' Int?)?  -- legacy *m..n shorthand
             | '{' Int (',' Int?)? '}'  -- explicit range
             | '{' ',' Int '}'          -- upper-only
             | '+' | '*' | '?'          -- shorthand desugars

Variable ::= Identifier
Type ::= Identifier ( '|' Identifier )*  -- single type or union (e.g. :R, :R1|R2|R3)
```

**Undirected matching over a self-loop.** A self-loop's two orientations are the
same binding — its source and target are the same node — so an undirected slot
matches it **once**:

```cypher
CREATE (l:Looper)-[:LOOP]->(l)
MATCH ()-[]-() RETURN count(*)      -- 1, not 2
MATCH (l:Looper)--() RETURN count(*) -- 1
```

An ordinary relationship genuinely has two orientations under an undirected slot
and still yields both, so `MATCH ()-[]-()` over `(a)-[:T]->(b)` counts 2.

**Relationship isomorphism, scoped to the clause.** Two relationship slots of one
`MATCH` clause never bind the same relationship, so a pattern cannot walk back
over the edge it arrived on:

```cypher
CREATE (a:A)-[:T]->(b:B)                            -- one relationship
MATCH (x)-[]-(y)-[]-(z) RETURN count(*)             -- 0; both slots would need it
MATCH (x)-[r1]-(y)-[r2]-(z) RETURN count(*)         -- 0; naming changes nothing
```

The scope is the **clause**, and the boundary matters in both directions:

- It **spans comma-separated parts** of one clause, which openCypher treats as a
  single pattern: on the graph above, `MATCH (x)-[r1]->(y), (p)-[r2]->(q)`
  returns 0 rows, and only with two distinct relationships available does it
  return the pairs of distinct ones.
- It does **not span separate clauses**: `MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q)`
  returns 1 row on the same graph, because each clause binds independently.
  Re-using a relationship variable in a later clause is likewise legal — it joins
  on that relationship.

Enforcement lives on the `Expand` operator (each hop carries its clause's scope
id and refuses a relationship an earlier hop of that scope consumed), so it
covers anonymous slots as well as named ones. `EXISTS { … }`, pattern
comprehensions and variable-length segments enforce the same rule through their
own traversal code.

**Re-using one relationship variable inside a pattern is rejected.** Because the
two slots must bind different relationships, `MATCH (a)-[r]->()-[r]->(a)` is
unsatisfiable, and Cypher rejects it instead of quietly returning no rows —
a `SyntaxError` carrying the openCypher detail token
`RelationshipUniquenessViolation`, raised at compile time before any traversal.
The check is scoped to `MATCH` patterns; a rebind in `CREATE`/`MERGE` is a
different rule and reports `VariableAlreadyBound`.

> **Remaining gap.** Isomorphism is not enforced *between* a single-hop slot and
> a variable-length or quantified segment of the same clause (e.g.
> `MATCH (a)-[r]->(b)-[*]->(c)`): each carries its own rule, and the shared scope
> stops at the operator boundary.

**Comma-separated parts bind independently, and a later clause cannot unbind
them.** Each part of `MATCH (a:A), (z:Z)` gets its own driving scan, and the rows
are the cartesian product of the parts. A variable bound that way stays bound for
every later clause — including one that uses it as a relationship target:

```cypher
CREATE (a:A {n: 'a'}), (z:Z {n: 'z'}), (b:B), (a)-[:T]->(b)   -- a has a :T edge, not to z
MATCH (a:A), (z:Z)
OPTIONAL MATCH (a)-[r:T]->(z)
RETURN a.n, z.n, r IS NULL                                    -- 'a', 'z', true
```

`z` keeps its binding and only `r` is `NULL`, because the optional hop closes over
an already-bound target rather than producing it. The same rule drives
`OPTIONAL MATCH … WITH other WHERE r IS NULL`, the standard "rows with no such
edge" idiom.

**A label predicate on an already-bound variable is enforced as a filter.** When a
later required clause re-states a variable with a label —
`MATCH (a)-[:T]->(b) MATCH (b:B)` — the binding is kept (it is not re-scanned,
which would discard the traversal result) and `:B` is checked against the bound
node, so a `b` without that label yields no rows.

> **Remaining gap.** In an `OPTIONAL MATCH`, a label re-stated on an already-bound
> variable is not enforced: expressing it would have to leave the row NULL-padded
> rather than drop it, which a filter cannot do.

**Label/type colon whitespace.** openCypher permits whitespace around the colon
in the label and relationship-type positions, so `(dur2: Duration2)`,
`(v : A : B)` and `-[r: KNOWS]->` are all accepted and equivalent to their
unspaced forms. The tolerance is scoped to those pattern positions: the
property-map colon (`{k: v}`) and the expression-level label predicate
(`WHERE n:Label`) are parsed by separate code paths and are unchanged.

### WHERE Clause

> **Clause-ordering rule** (Neo4j 2025.09.0 parity, enforced since
> `phase3_unwind-where-neo4j-parity`): `WHERE` is only valid
> immediately after a `MATCH`, `OPTIONAL MATCH`, or `WITH`. It is
> **not** a standalone top-level clause — `UNWIND … AS x WHERE …`,
> `CREATE (…) WHERE …`, and any other non-MATCH/WITH producer
> followed by `WHERE` reject with a syntax error listing the
> allowed follow-up clauses. Migrate by inserting a `WITH <vars>`
> pass-through projection:
>
> ```cypher
> -- reject: bare WHERE after UNWIND
> UNWIND [1, 2, 3, 4, 5] AS x WHERE x > 2 RETURN x
>
> -- accept: WITH x projects through, WHERE attaches to WITH
> UNWIND [1, 2, 3, 4, 5] AS x WITH x WHERE x > 2 RETURN x
> ```

```cypher
-- Property equality
WHERE n.name = 'Alice'

-- Comparison operators
WHERE n.age > 25
WHERE n.age >= 18
WHERE n.age < 65
WHERE n.age <= 100
WHERE n.age != 0

-- Boolean operators
WHERE n.age > 25 AND n.city = 'NYC'
WHERE n.active = true OR n.premium = true
WHERE NOT n.deleted

-- IN operator
WHERE n.status IN ['active', 'pending']
WHERE n.age IN [18, 21, 65]

-- IS NULL / IS NOT NULL
WHERE n.email IS NOT NULL
WHERE n.deleted_at IS NULL

-- String operations ✅ IMPLEMENTED
WHERE n.name STARTS WITH 'Al'
WHERE n.email ENDS WITH '@example.com'
WHERE n.bio CONTAINS 'engineer'

-- Pattern matching (regex) ✅ IMPLEMENTED
WHERE n.email =~ '.*@example\.com'
```

**Expression Syntax**:
```
Expr ::= Literal 
       | Property
       | Expr BinOp Expr
       | UnOp Expr
       | Expr 'IN' List
       | Expr 'IS' 'NULL'
       | Expr 'IS' 'NOT' 'NULL'

Property ::= Variable '.' Identifier

BinOp ::= '=' | '!=' | '>' | '>=' | '<' | '<=' | 'AND' | 'OR'

UnOp ::= 'NOT' | '-'

Literal ::= Number | String | Boolean | Null

List ::= '[' (Literal (',' Literal)*)? ']'
```

### RETURN Clause

```cypher
-- Return nodes
RETURN n

-- Return properties
RETURN n.name, n.age

-- Return relationships
RETURN r

-- Aliasing
RETURN n.name AS name, n.age AS age

-- Distinct results
RETURN DISTINCT n.city

-- All properties
RETURN n.*

-- Expressions ✅ IMPLEMENTED
RETURN n.age + 1 AS next_age
RETURN n.price * 1.1 AS price_with_tax
RETURN 1 + 1 AS result  -- Literal expressions
```

**Return Syntax**:
```
Return ::= 'RETURN' ('DISTINCT')? ReturnItem (',' ReturnItem)*

ReturnItem ::= Expr ('AS' Identifier)?
```

### ORDER BY Clause

```cypher
-- Single column
ORDER BY n.age

-- Multiple columns
ORDER BY n.city, n.age DESC

-- Ascending (default)
ORDER BY n.name ASC

-- Descending
ORDER BY n.created_at DESC
```

**Order Syntax**:
```
OrderBy ::= 'ORDER' 'BY' OrderItem (',' OrderItem)*

OrderItem ::= Expr ('ASC' | 'DESC')?
```

**A sort key does not have to be projected.** `ORDER BY` sees the scope the
projection consumed, not only the columns it produced, so both of these sort:

```cypher
MATCH (n:Person) RETURN n.name ORDER BY n.age      -- key not in the output
UNWIND [3, 1, 2] AS v RETURN v ORDER BY v * -1     -- key is an expression
```

Such a key is carried through the projection as an internal column and dropped
again once the rows are ordered, so it never appears in the result. It is not
available after `DISTINCT` or an aggregating projection — Cypher puts the
pre-projection variables out of scope there, and those forms sort only by what
they project.

**Ordering across types is a total order, not a value comparison.** When a sort
key holds values of different types, they are ordered by type first, ascending:

```
MAP < NODE < RELATIONSHIP < LIST < PATH < STRING < BOOLEAN < NUMBER < NaN < null
```

`DESC` is the exact reverse of that sequence, which puts `null` **first**
descending. Within one type the ordering is the type's own (numeric,
lexicographic, element-wise for lists). This total order belongs to `ORDER BY`
alone: the comparison operators do not derive from it, so `1 < 'text'` is not a
verdict from this table.

> **Note.** `NaN`'s slot between `NUMBER` and `null` is currently unreachable —
> the value representation cannot hold a non-finite float. Tracked separately.

### Comparison and equality across types

The ordering operators are defined only **within** a type; equality is defined
across all of them. The two rules differ, deliberately:

```cypher
RETURN 1 < 'text'      -- null  (incomparable: no verdict, not false)
RETURN true < 1        -- null
RETURN [1] < 1         -- null
RETURN 1 < 3.14        -- true  (INTEGER and FLOAT are one numeric kind)

RETURN 1.0 = '1.0'     -- false (a number never equals a string, however spelled)
RETURN 1 = 1.0         -- true
RETURN 1 <> 1.0        -- false (always the exact negation of `=`)
```

`null` on either side yields `null` for every one of these operators. Under
`WHERE`, a `null` result drops the row — which is why an incomparable pair
filters nothing rather than filtering by accident.

Inline property matching (`MATCH (n {id: 1})`) is equality, so it follows the
same rule: `{id: '1'}` does not match a node whose `id` is the number `1`.

Temporal values are a documented exception to "a string is never a number's
peer": they are stored as canonical ISO-8601 strings, so nothing downstream can
tell `'PT10H'` from `duration('PT10H')`, and the two compare as the durations
they spell.

> **Remaining gaps in this area.** Structural comparison *within* a type is not
> yet fully conformant — map equality's own three-valued rule (a differing key
> set is `false`, an equal key set with a `null` on either side is `null`) and
> parts of list comparison. And `WHERE` has a third comparison implementation
> that coerces operands to numbers, so `WHERE n.number < 'text'` still keeps the
> row where a projected `n.number < 'text'` correctly yields `null`.

### LIMIT Clause

```cypher
-- Limit results
LIMIT 10

-- Combined with ORDER BY
ORDER BY n.score DESC LIMIT 100

-- Zero is a legal count and returns nothing
LIMIT 0
```

**Limit Syntax**:
```
Limit ::= 'LIMIT' (Integer | Parameter)
```

**Arguments.** The count is a non-negative integer literal or a `$param`
bound to one; both `SKIP` and `LIMIT` accept the same forms. `LIMIT 0`
returns zero rows, and a `SKIP` past the last row leaves nothing — an empty
result is the answer, not a signal that the clause was ignored.

Anything that cannot be a row count is rejected before execution, with the
openCypher error kind in the message:

| Written | Error kind |
|---|---|
| `LIMIT -1`, `SKIP -1` | `NegativeIntegerArgument` |
| `LIMIT 1.5`, `LIMIT 'x'`, `LIMIT true` | `InvalidArgumentType` |
| `LIMIT n.age`, `SKIP size(n.list)` | `NonConstantExpression` |

A parameter's value is only known at request time, so a `$param` bound to a
negative number or a non-integer is not a static error; it resolves to no
count and the clause does not restrict the stream.

### SKIP Clause (V1)

```cypher
-- Skip first N results
SKIP 10

-- Pagination
SKIP 20 LIMIT 10  -- page 3, size 10
```

**Supported contexts.** SKIP is applied in the standard openCypher `ORDER BY` → `SKIP` → `LIMIT` pipeline order on pattern-less queries, procedure YIELD projections, and pattern-driven `MATCH` queries (including aggregation projections, `WITH` pipelines, and post-`UNION` projections):
- ✅ `CALL db.labels() YIELD label RETURN label SKIP 1 LIMIT 10`
- ✅ `RETURN 1 SKIP 1`
- ✅ `UNWIND [1, 2, 3] AS x RETURN x SKIP 1`
- ✅ `MATCH (n) RETURN n.v AS v ORDER BY v SKIP 1` — drops the first sorted row
- ✅ `MATCH (n) RETURN n.v AS v ORDER BY v SKIP 1 LIMIT 2` — pagination over the sorted set
- ✅ `MATCH (n:N) RETURN n.v AS v, count(*) AS c ORDER BY v SKIP 2` — after an aggregation projection
- ✅ `MATCH ... RETURN v UNION MATCH ... RETURN v ORDER BY v SKIP 2` — SKIP applies to the merged result

Pair `SKIP` with `ORDER BY` for deterministic pagination — without an explicit ordering the rows dropped are implementation-defined. On a post-`UNION` projection, a `SKIP`/`LIMIT` written *without* an accompanying `ORDER BY` binds to the nearest `RETURN` (the right-hand UNION arm) rather than the merged result, matching openCypher clause attachment; add `ORDER BY` after the final `UNION` to page the combined output.

**Attachment to `WITH`.** `ORDER BY` / `SKIP` / `LIMIT` bind to the
projecting clause they are written after, and a tail on a `WITH` cuts the
stream *at that point* — every later clause, aggregations included, sees
only the surviving rows:

- ✅ `UNWIND [1, 2, 3, 4, 5] AS x WITH x LIMIT 2 RETURN count(x)` → `2`
- ✅ `UNWIND [1, 2, 3, 4, 5] AS x WITH x SKIP 3 RETURN sum(x)` → `9`
- ✅ `MATCH (p:Person) WITH p ORDER BY p.age LIMIT 2 RETURN p.name` — the two youngest
- ✅ `UNWIND [1, 2, 3, 4, 5] AS x WITH x LIMIT 2 RETURN x LIMIT 4` → `1, 2` — each tail applies in turn, the tighter one wins

A `WITH` tail may sort by a key the `WITH` does not project (`WITH p ORDER BY
p.age`); the key is carried across the projection as an internal column and
dropped once the rows are ordered. The one exception is `WITH DISTINCT`,
where an extra column would change which rows count as duplicates — sort
there by a projected alias (`WITH DISTINCT p.age AS age ORDER BY age`).

### Aggregations

```cypher
-- Count
RETURN COUNT(*)
RETURN COUNT(n)
RETURN COUNT(DISTINCT n.city)

-- Sum
RETURN SUM(n.age)

-- Average
RETURN AVG(n.age)

-- Min/Max
RETURN MIN(n.age), MAX(n.age)

-- Collect ✅ IMPLEMENTED
RETURN COLLECT(n.name) AS names
RETURN COLLECT(DISTINCT n.city) AS cities

-- Group by
MATCH (p:Person)-[:LIKES]->(prod:Product)
RETURN prod.category, COUNT(*) AS likes
ORDER BY likes DESC
```

**Aggregation Syntax**:
```
AggFunc ::= 'COUNT' '(' ('DISTINCT')? Expr ')'
          | 'SUM' '(' Expr ')'
          | 'AVG' '(' Expr ')'
          | 'MIN' '(' Expr ')'
          | 'MAX' '(' Expr ')'
          | 'COLLECT' '(' Expr ')'  // V1
```

**Empty and multi-hop patterns.** An aggregation over a pattern with no matches
returns one row with the identity value (`count(*)`/`count(x)` = 0). This holds
for multi-hop relationship patterns too: `count(*)` over
`MATCH (a)-[:R1]->(b)-[:R2]->(c)` reports the true number of matching paths — `0`
when any hop is absent — never a phantom count.

**Grouping keys.** In an aggregating projection every non-aggregate item is a
grouping key, whatever its expression shape — a variable, a literal, an
arithmetic expression or a function call, not only a property access on a graph
variable — and each keeps its column in the result:

```cypher
UNWIND [1, 1, 2] AS v WITH v AS k, count(*) AS c RETURN k, c  -- k,c = 1:2, 2:1
UNWIND [1, 1, 2] AS v WITH 9 AS k, count(*) AS c RETURN k, c  -- k,c = 9:3
MATCH (p:Person) RETURN p.city AS city, count(*) AS n         -- one row per city
```

**Aggregates nested in an expression.** An aggregate may sit anywhere inside a
larger expression. It is evaluated once per group and the enclosing expression is
computed afterwards, over the aggregated value:

```cypher
MATCH (n:P) RETURN count(*) + 1                             -- one row
MATCH (n:P) RETURN sum(n.x) * 2
MATCH (n:P) RETURN [count(*)]
MATCH (n:P) RETURN CASE WHEN count(*) > 0 THEN 'yes' ELSE 'no' END
MATCH (n:P) RETURN n.x AS k, head(collect(n.x)) AS h        -- k stays a column
```

This holds over empty input too: the aggregate still yields its identity value
and the wrapping expression is computed over it, so `MATCH (a:Absent) RETURN
count(a) > 0` returns one row containing `false` — never zero rows.

**Fully-anonymous relationships.** A fully-anonymous relationship pattern with no
node or relationship variables — `MATCH ()-[:TYPE]->() RETURN count(*)` — counts
every matching relationship, not just the unique target nodes. Prior behavior under-counted by deduplicating rows incorrectly; the fix ensures each relationship is counted exactly once.

**Argument-domain & overflow errors.** These operations return a bounded Cypher
error (never a panic or a silently wrapped value) on out-of-range input:

- `percentileCont(expr, p)` / `percentileDisc(expr, p)` require `p` in `[0.0, 1.0]`
  (a value outside the range, or `NaN`, is rejected).
- Temporal arithmetic — `date`/`datetime`/`localdatetime` `±` `duration` — errors
  when the result would leave the representable date range instead of panicking.
- Duration arithmetic — `duration ± duration` — errors on component overflow
  instead of wrapping.

## KNN Procedures (MVP)

### vector.knn

```cypher
-- Basic KNN search
CALL vector.knn('Person', $embedding, 10)
YIELD node, score
RETURN node.name, score
ORDER BY score DESC

-- KNN with filters
CALL vector.knn('Person', $embedding, 10)
YIELD node, score
WHERE node.active = true
RETURN node.name, score

-- KNN-seeded traversal
CALL vector.knn('Person', $embedding, 10)
YIELD node AS n
MATCH (n)-[:WORKS_AT]->(c:Company)
RETURN n.name, c.name, score
```

**Signature**:
```
vector.knn(
  label: String,        // Node label to search
  vector: List<Float>,  // Query embedding
  k: Integer            // Number of neighbors
) YIELDS (
  node: Node,          // Matched node
  score: Float         // Similarity score (0.0-1.0)
)
```

## Full-Text Procedures (V1)

### text.search

```cypher
-- Full-text search
CALL text.search('Person', 'bio', 'software engineer', 10)
YIELD node, score
RETURN node.name, node.bio, score
ORDER BY score DESC
```

**Signature**:
```
text.search(
  label: String,      // Node label
  key: String,        // Property key
  query: String,      // Search query
  limit: Integer      // Max results
) YIELDS (
  node: Node,        // Matched node
  score: Float       // Relevance score (BM25)
)
```

## Write Operations ✅ IMPLEMENTED

### CREATE

```cypher
-- Create node
CREATE (n:Person {name: 'Alice', age: 30})
RETURN n

-- Create relationship
MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
CREATE (a)-[r:KNOWS {since: 2020}]->(b)
RETURN r

-- Create multiple
CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
CREATE (a)-[:KNOWS]->(b)

-- Inline node creation in MATCH...CREATE patterns
MATCH (a:Person {name: 'Alice'})
CREATE (a)-[r:KNOWS]->(b:Person {name: 'Bob', age: 30})
RETURN a, r, b

-- Multi-hop chains with mixed bound and inline nodes
MATCH (a:Person {name: 'Alice'})
CREATE (a)-[:KNOWS]->(b:Person {name: 'Bob'})
        -[:WORKS_AT]->(c:Company {name: 'Acme'})
RETURN a, b, c
```

**A property written with a `null` value is ABSENT, not present-and-null.** The
key is never stored, on nodes and relationships alike, and the `+properties`
counter agrees because it is derived from the same filtered map:

```cypher
CREATE (n {id: 12, name: null})
RETURN keys(n)                    -- ['id'] — no 'name' key at all
                                  -- side effects: +properties 1
```

Reading it back is indistinguishable from a stored null (`n.name IS NULL` is
`true` either way) — `keys(n)` / `properties(n)` are what reveal the difference.
The same rule governs the update paths: `SET n.p = null` and `SET n += {p: null}`
**remove** an existing key (`-properties 1`) and are a no-op on an absent one, and
a whole-entity replace (`SET n = {…}`) never stores a null-valued key. Records
written by earlier versions may still carry a null-valued key on disk; reads
tolerate them, and nothing rewrites them.

A `null` in a `MERGE` pattern is a different matter — it makes the pattern
unsatisfiable rather than absent, so `MERGE (n {name: null})` is rejected
("Cannot merge node using null property value"), matching Neo4j.

**Inline node creation in relationship patterns (MATCH…CREATE).** When a node appears in a relationship
pattern within a `CREATE` clause following a `MATCH`, it is created inline during the relationship
write if the node's variable is unbound. The node is written immediately before the relationship,
ensuring the full pattern (nodes and edges) persists atomically. Anonymous nodes (no explicit
variable) can also anchor relationships. Multi-hop chains combine bound nodes (from the preceding
`MATCH`) and inline-created nodes seamlessly.

**CREATE semantics guarantee: each pattern element produces exactly one node.** A `CREATE` statement that
combines a relationship pattern with write clauses (`SET`, `REMOVE`, `MERGE`, or `FOREACH` in the same
query) produces exactly one instance of each node, even when the node appears in an inlined relationship
pattern. Prior versions created a phantom duplicate in this scenario; the fix (3.0.0) ensures pattern
materialization is atomic and non-duplicating. All query variables bind to the connected node, not an
orphan. See `docs/data-corruption/CREATE-relationship-phantom-target-audit.md` for historical context
and data-safety migration guidance.

**Index and constraint maintenance (both CREATE forms).** A node created by a
bare `CREATE` and one created by a `MATCH…CREATE` are treated identically:

- **Typed property index** — the new node is inserted into the typed property
  B-tree for every registered `(label, key)` index, so it is immediately
  visible to `NodeIndexSeek`, `find_exact`, and the index-backed `MERGE`
  existence check (no `MERGE`-created duplicates of a `MATCH…CREATE` node).
- **Composite / `NODE KEY` index** — the new node's tuple is inserted into every
  registered composite B-tree matching its labels.
- **Extended constraints** — `NODE KEY` (each key present, non-null, tuple
  unique) and property-type constraints are enforced. A duplicate `NODE KEY`
  tuple is rejected whether the duplicate comes from an earlier statement or an
  earlier node in the same multi-node `CREATE`.

A `CREATE` statement that violates an extended constraint is rejected **as a
whole** — no partial write survives (relationships created by the statement are
rolled back along with the nodes). Single-column `UNIQUE` / `EXISTS` constraints
are additionally rejected up front by the executor's local check.

### SET

```cypher
-- Set property
MATCH (n:Person {name: 'Alice'})
SET n.age = 31
RETURN n

-- Set multiple properties
MATCH (n:Person {name: 'Alice'})
SET n.age = 31, n.city = 'NYC'

-- Add label
MATCH (n:Person {name: 'Alice'})
SET n:Employee

-- Parenthetical target — semantically identical to the bare-variable form
MATCH (n:Person {name: 'Alice'})
SET (n).age = 31

-- Merge a map into the existing property bag: keys in the map overwrite
-- (non-null values) or remove (explicit null values); keys NOT mentioned
-- in the map are left untouched.
MATCH (n:Person {name: 'Alice'})
SET n += {age: 31, city: 'NYC'}

-- Whole-entity property replace: every property currently on the node
-- that is NOT a key of the map is removed, then the map's keys are
-- applied (an explicit null value in the map also removes that key).
-- The RHS may be a map literal, a bound node/map variable (copies that
-- entity's current property bag), or a map parameter (`SET n = $props`).
MATCH (n:Person {name: 'Alice'})
SET n = {name: 'Alice', age: 31}

-- Function calls, including temporal constructors, evaluate on the SET RHS
-- (and inside a `+=` map), against the target's own current properties.
MATCH (n:Person {name: 'Alice'})
SET n.name_upper = toUpper(n.name),
    n.seen = duration({days: 1}),
    n.born = date('1990-05-17')
```

**Temporal values persist in one representation.** A temporal property is stored
as its canonical ISO-8601 `STRING` regardless of which clause wrote it — `CREATE`
and `SET` converge on the same storage boundary, so two nodes holding the same
logical value are byte-identical on disk (which is what lets an index or
constraint be built over a temporal column). Values written by older versions in
the executor's internal tagged form still read back correctly: reads render the
tag, only writes changed.

**A write query may RETURN several variables, and may carry a `WITH` between
writes.** Both were rejected outright until recently:

```cypher
MERGE (a:Product {name: 'A'}) MERGE (b:Product {name: 'B'})
RETURN a.name, b.name            -- one row, both bound

MERGE (n:Product {name: 'U'}) WITH n
MERGE (n2:Product {name: 'U'})   -- matches n, does not duplicate
RETURN count(DISTINCT n)
```

A multi-variable `RETURN` is answered by materialising the write's bindings into a
read query and letting the normal executor build the rows, so its row semantics
are the executor's rather than a second model. A `WITH` between write clauses acts
as a **scope cut**: it keeps the variables it projects (optionally renaming them)
and drops the rest.

**Current limitations:**
- A `WITH` in a write query may project **bare variables only** (`WITH n`,
  `WITH n AS m`). A projected property, expression or aggregation has no binding
  to carry forward and is rejected with a message saying so.
- Whole-entity replace (`SET x = {...}`) and the parenthetical target form
  are implemented for **node** variables only; `SET r = {map}` on a
  relationship variable is not yet supported (only the `+=` merge form is —
  `SET r += {map}`).
- `OPTIONAL MATCH` combined with any write clause (`SET`/`REMOVE`/`MERGE`/
  `FOREACH`) is rejected outright by the write path. This means the
  openCypher `Set4` conformance-suite scenario that expects a `SET` on an
  unmatched `OPTIONAL MATCH` target to be a silent no-op is not yet
  reachable — it needs `OPTIONAL MATCH` support in the write pipeline
  first, which is a separate, broader gap than `SET` itself.

### DELETE

```cypher
-- Delete node (must delete relationships first)
MATCH (n:Person {name: 'Alice'})
DELETE n

-- Delete relationship
MATCH (a:Person)-[r:KNOWS]->(b:Person)
WHERE a.name = 'Alice'
DELETE r

-- Delete with DETACH (deletes relationships automatically) ✅ IMPLEMENTED
MATCH (n:Person {name: 'Alice'})
DETACH DELETE n
```

**Relationship-existence guard** (`phase0_fix-delete-node-dangling-relationships`):
a non-`DETACH` `DELETE` of a node that still has ANY live relationship —
**outgoing OR incoming** — fails with an error; use `DETACH DELETE`. The check
is enforced centrally in `Engine::delete_node`, so it applies uniformly to
every entry point (Cypher `MATCH…DELETE` and `FOREACH…DELETE`, REST, RPC,
RESP3), not just Cypher. This closes a prior gap where an incoming-only node
(which keeps `first_rel_ptr == 0`, since that pointer tracks outgoing edges
only) slipped past the guard and was hard-deleted under a live edge, leaving a
dangling relationship. Invariant: no live relationship record may reference a
deleted node.

### REMOVE

```cypher
-- Remove property
MATCH (n:Person {name: 'Alice'})
REMOVE n.age

-- Remove label
MATCH (n:Person:Employee {name: 'Alice'})
REMOVE n:Employee
```

## Reserved `_id` Property and `ON CONFLICT` Clause

**Nexus Extension**: The `_id` property is reserved and stores the external identifier of a node. This is a Nexus-specific feature with no direct Neo4j equivalent.

### CREATE with External ID

```cypher
-- Create with hash-based external id
CREATE (f:File {_id: 'sha256:abc123def456…', path: '/data/file.txt', size: 1024})
RETURN f._id, f.path

-- Create with UUID
CREATE (d:Document {_id: 'uuid:550e8400-e29b-41d4-a716-446655440000', title: 'Report'})

-- Create with string external id
CREATE (u:User {_id: 'str:user-42', name: 'Alice'})

-- Create with parameterized external id
CREATE (n:Node {_id: $external_id, data: $data})
RETURN n._id
```

### Accepted `_id` Formats

The `_id` property accepts string literals or parameters with a prefixed format:

- `'sha256:<hex_string>'` — 32-byte SHA-256 hash as hex (64 chars)
- `'sha512:<hex_string>'` — 64-byte SHA-512 hash as hex (128 chars)
- `'blake3:<hex_string>'` — 32-byte BLAKE3 hash as hex (64 chars)
- `'uuid:<canonical>'` — UUID in canonical RFC 4122 format (e.g., `550e8400-e29b-41d4-a716-446655440000`)
- `'str:<value>'` — Arbitrary string key ≤ 256 bytes UTF-8
- `'bytes:<hex_string>'` — Opaque binary value ≤ 64 bytes as hex

### ON CONFLICT Clause

When a node is created with `_id`, the optional `ON CONFLICT` modifier controls the behavior if an external ID already exists:

```cypher
-- ERROR (default): fail if external id exists
CREATE (n:File {_id: 'sha256:abc…', path: '/file.txt'})
RETURN n._id

-- MATCH: return the existing node, discard new properties
CREATE (n:File {_id: 'sha256:abc…', path: '/newpath'}) ON CONFLICT MATCH
RETURN n._id

-- REPLACE: return the existing node, update properties
CREATE (n:File {_id: 'sha256:abc…', path: '/newpath'}) ON CONFLICT REPLACE
RETURN n._id
```

**Conflict Policy Semantics**:

- **ERROR** (default): If a node with the same `_id` already exists, the operation fails with `ExternalIdConflict`. No node is created or modified.
- **MATCH**: If a node with the same `_id` already exists, return that node unchanged. Any properties from the CREATE pattern are discarded. Idempotent for read-like checks.
- **REPLACE**: If a node with the same `_id` already exists, update its properties with the provided values. The internal node ID remains unchanged. Labels are preserved unless explicitly redefined in the CREATE pattern.

### Projecting `_id`

```cypher
-- Project the external id
MATCH (n:File {_id: 'sha256:abc…'})
RETURN n._id, n.path

-- Project null when external id is absent
MATCH (n:File)
RETURN n._id  -- null for nodes without external id
```

### Index Seek on `_id` Predicates

The query planner automatically uses the external-ID index when filtering by `_id`:

```cypher
-- Index seek (fast)
MATCH (n {_id: 'sha256:abc…'})
RETURN n

-- Parameterized (also index seek)
MATCH (n {_id: $external_id})
RETURN n

-- WHERE clause variant (index seek)
MATCH (n) WHERE n._id = 'sha256:abc…'
RETURN n
```

### Index Seek on Property Predicates

The query planner uses property indexes when available for inline property predicates in node patterns:

**Constant inline predicates** (already seekable):
```cypher
-- Constant literal → index seek
MATCH (n:Person {id: 42})
RETURN n

-- Constant parameter → index seek
MATCH (n:Person {id: $userId})
RETURN n
```

**Correlated inline predicates** (row-local expressions from UNWIND/WITH):
```cypher
-- Row-local property access from UNWIND → index seek per driving row
UNWIND $rows AS r
MATCH (a:Person {id: r.s})
RETURN a

-- Multi-step: correlated predicate with multiple rows
UNWIND [10, 20, 30] AS user_id
MATCH (u:Person {id: user_id})
CREATE (u)-[:VISITED]->(place:Place {name: 'New Location'})

-- Batch endpoint resolution via correlated predicates
UNWIND $edges AS edge
MATCH (a:Person {id: edge.from_id}), (b:Person {id: edge.to_id})
CREATE (a)-[:KNOWS]->(b)
```

**WHERE-clause predicates** (every form below seeks when an index exists):
```cypher
-- WHERE-clause equality with literal → index seek
MATCH (n:Person) WHERE n.age = 30
RETURN n

-- WHERE-clause equality with parameter → index seek, key resolved at
-- execution time (the predicate is kept as a residual filter)
MATCH (n:Person) WHERE n.age = $age
RETURN n

-- WHERE-clause range comparison with literal → index range seek
-- (>, >=, <, <=, and the mirrored `30 < n.age`)
MATCH (n:Person) WHERE n.age > 30
RETURN n

-- IN over a literal list → union of point seeks
MATCH (n:Person) WHERE n.age IN [30, 40, 50]
RETURN n

-- STARTS WITH a literal prefix → index prefix seek
MATCH (n:Person) WHERE n.name STARTS WITH 'A'
RETURN n

-- CONTAINS → full-table filter (an unanchored substring match cannot be
-- served by an ordered index)
MATCH (n:Person) WHERE n.name CONTAINS 'li'
RETURN n
```

**Fallback and limitations**:
- If no index exists on the `(label, property)` pair, both constant and correlated predicates fall back to a label scan followed by property filtering.
- WHERE-clause equality comparisons with **literal values** (e.g., `WHERE n.property = 30`) now use index seeks when an index exists on the property.
- WHERE-clause **range comparisons** with a literal (`>`, `>=`, `<`, `<=`, and the mirrored `30 < n.age`) now use an index **range seek** when an index exists (exclusive `>`/`<` exclude the threshold; residual filters still run). One bound lifts to the seek; a second bound (`age > 10 AND age < 40`) stays a residual filter.
- WHERE-clause **`IN`** over a literal list uses a **union of point seeks** (one per element). `NULL` elements are dropped — they can never make the comparison true — so `IN []` and `IN [null]` correctly match nothing. A `$parameter` list (`IN $ages`) has no plan-time elements and stays a label scan.
- WHERE-clause **`STARTS WITH`** a literal prefix uses an index **prefix seek** (the contiguous run of string keys sharing the prefix). Only `n.prop STARTS WITH 'x'` seeks; the reversed `'x' STARTS WITH n.prop` asks a different question and stays a scan.
- WHERE-clause equality against a **`$parameter`** uses a **parameter seek** that resolves the key from the query envelope at execution time (no driving rows required). If the parameter is bound to a list/map, or is absent from the envelope, the operator falls back to a label scan — which is why this is the one seek that keeps its predicate as a residual filter.
- Numeric seek keys probe both `Integer` and `Float` index entries, because Cypher compares `10` and `10.0` equal while the B-tree keys them separately.
- **`CONTAINS`** still evaluates the filter after a label scan even with an index, and keeps emitting `Nexus.Performance.UnindexedPropertyAccess`.
- Predicate values that are function calls or complex expressions are not index-eligible and trigger a label scan.

**Performance impact**:
- Constant inline predicates: O(log N) or O(matches) via index seek
- Correlated inline predicates with an index: O(R·log N) where R is the driving row count (one seek per row)
- Correlated inline predicates without an index: O(R·N) label scan (quadratic behavior in the absence of indexes)

### Write Forms Honouring External ID

Reserved property `_id` is now honoured by all major write forms:

**Fast path: MERGE constrained by external ID**

```cypher
-- MERGE uses external-id index (consulted before property-pattern search)
-- Returns existing node if _id matches; creates if absent
MERGE (n:File {_id: 'sha256:abc…'})
ON CREATE SET n.imported_at = timestamp()
ON MATCH SET n.synced_at = timestamp()
RETURN n._id

-- Idempotent: same _id always resolves to the same node
MERGE (n:User {_id: 'uuid:1234…'})
ON CREATE SET n.created_at = timestamp()
ON MATCH SET n.last_seen = timestamp()
RETURN n._id
```

**All supported write forms now preserve `_id`**:

- `CREATE (n:L {_id:'str:x', ...})` — creates node with external ID (always worked)
- `CREATE (n:L {_id:'str:x', ...}) SET ...` — creates with external ID, then applies SET (fixed)
- `MERGE (n:L {_id:'str:x'}) ON CREATE SET ... ON MATCH SET ...` — external-id index consulted first, TOCTOU-safe (fixed)
- `UNWIND ... CREATE (n {_id: $id, ...})` — where `$id` is a literal string or parameter (fixed)
- `UNWIND ... MERGE (n:L {_id: $id})` — delegates to MERGE path (fixed)
- **NEW: Relationship MERGE with per-endpoint external IDs**:
  ```cypher
  -- Each endpoint may carry its own _id (or none)
  MERGE (a:Person {_id:'uuid:alice'})
  -[r:KNOWS {start_year: 2020}]->
  (b:Person {_id:'uuid:bob'})
  ON CREATE SET r.met_at = timestamp()
  ```

**Explicit limitations** (deliberate design):

1. **Per-row `_id` from UNWIND row is a parse error**: `UNWIND $rows AS r CREATE (n {_id: r.id})` is rejected. Only a literal string or `$param` is accepted. Rationale: a constant `_id` across rows would collide from row 2 anyway; rejected explicitly rather than silently dropped.

2. **At most one node in a CREATE pattern carries `_id`**: `CREATE (a {_id: 'str:a'}) (b {_id: 'str:b'})` is a parse error (`_id may only appear once`). Only MERGE patterns support per-endpoint `_id` (via separate MERGE clauses or relationship endpoints).

3. **Invalid `_id` values surface an explicit error** (never silently ignored):
   - Missing or unknown prefix: use one of `blake3:`, `sha256:`, `sha512:`, `uuid:`, `str:`, `bytes:`
   - Non-string value: `_id` must be a string or parameter
   - Unresolved `$param`: returns a Cypher error

4. **Relationships have no external IDs** (by design): only node endpoints carry `_id`. A relationship record itself cannot be looked up by external ID.

**Match-before-pattern-search semantics**:

When a MERGE pattern includes `_id`, the external-id index is consulted **before** the property-pattern search. This makes the external ID the stronger key and closes a TOCTOU (time-of-check-time-of-use) window: if two concurrent MERGE requests target the same `_id`, one wins without either seeing a phantom duplicate.

## Query Examples

### Example 1: Social Network

```cypher
-- Find friends of friends
MATCH (me:Person {name: 'Alice'})-[:KNOWS]->(friend)-[:KNOWS]->(fof)
WHERE fof <> me AND NOT (me)-[:KNOWS]->(fof)
RETURN DISTINCT fof.name
LIMIT 10
```

### Example 2: Recommendation

```cypher
-- Recommend products based on what friends bought
MATCH (me:Person {name: 'Alice'})-[:KNOWS]->(friend)
MATCH (friend)-[:BOUGHT]->(product:Product)
WHERE NOT (me)-[:BOUGHT]->(product)
RETURN product.name, COUNT(*) AS friend_count
ORDER BY friend_count DESC
LIMIT 5
```

### Example 3: KNN + Graph

```cypher
-- Find similar people and their companies
CALL vector.knn('Person', $my_embedding, 20)
YIELD node AS similar, score
WHERE similar.age > 25
MATCH (similar)-[:WORKS_AT]->(company:Company)
RETURN similar.name, company.name, score
ORDER BY score DESC
LIMIT 10
```

### Example 4: Aggregation

```cypher
-- Top product categories by sales
MATCH (person:Person)-[:BOUGHT]->(product:Product)
RETURN product.category, COUNT(*) AS purchases, AVG(product.price) AS avg_price
ORDER BY purchases DESC
LIMIT 10
```

## Advanced Query Features ✅ IMPLEMENTED

### MERGE Clause

```cypher
-- Match or create node
MERGE (n:Person {email: 'alice@example.com'})
ON CREATE SET n.created = true
ON MATCH SET n.last_seen = datetime()
RETURN n

-- Relationship pattern with anonymous endpoints/relationship (creates or matches the whole pattern)
MERGE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})
ON CREATE SET a.joined = datetime()
ON MATCH SET a.seen = datetime()

-- ON CREATE/ON MATCH SET can target endpoint nodes even when relationship/endpoints are anonymous
MERGE (:Person {name: 'Alice'})-[:KNOWS]->(:Person {name: 'Bob'})
ON CREATE SET (a:Person {name: 'Alice'}).joined = datetime()  -- targets the endpoint by pattern
```

Relationship MERGE patterns support fully anonymous forms:
- **Anonymous relationship** (`-[:TYPE]->` with no variable `r`): the edge is created or matched as part of the whole pattern.
- **Anonymous endpoints** (`(:Label {props})` with no variable): nodes are created or matched; ON CREATE/ON MATCH SET items targeting those nodes now apply (previously silently filtered out for anonymous endpoints).
- **Mixed**: one or both endpoints may lack variables; the relationship may lack a variable; all three are supported independently. The whole pattern (all three elements) is created or matched as an atomic unit.

### WITH Clause

```cypher
-- Query piping
MATCH (p:Person)
WITH p, COUNT(*) AS connection_count
WHERE connection_count > 10
RETURN p

-- Pre-aggregation
MATCH (n:Person)
WITH n.city AS city, COUNT(*) AS count
WHERE count > 5
RETURN city, count
```

**An alias a `WITH` mints is a first-class binding for every clause downstream,
including another `WITH`.** Chained projections compose, and renaming does not
have to stop at one hop:

```cypher
UNWIND [5, 1, 4] AS i
WITH i AS a
WITH a AS b          -- b carries the value, whatever produced it
RETURN b             -- [5, 1, 4]
```

This holds regardless of what the first `WITH` renamed — an `UNWIND` variable, a
`MATCH` binding, or a property access — and each `WITH`'s own `WHERE` filters
against the projection it belongs to.

### OPTIONAL MATCH

```cypher
-- Left outer join
MATCH (n:Person)
OPTIONAL MATCH (n)-[:KNOWS]->(friend)
RETURN n, friend
```

#### Standalone OPTIONAL MATCH semantics (phase8_optional-match-empty-driver)

When `OPTIONAL MATCH` is the **first** clause of a query and no
prior driver (`UNWIND`, prior `MATCH`, etc.) has fed the pipeline,
the OPTIONAL contract demands one row with the optional variables
bound to `NULL` even when the scan produces no matches. Concrete
shapes:

```cypher
-- Empty label scan — returns one row with n = null
OPTIONAL MATCH (n:NonExistentLabel) RETURN n;

-- Property access on the null binding — returns one row, name = null
OPTIONAL MATCH (n:NonExistentLabel) RETURN n.name AS name;

-- Aggregation — returns one row, c = 0
OPTIONAL MATCH (n:NonExistentLabel) RETURN count(n) AS c;
```

When OPTIONAL MATCH follows a prior MATCH that itself produced
zero rows, the OPTIONAL does NOT re-introduce wrapped-NULL rows
— the prior MATCH already eliminated everything:

```cypher
-- Person is empty; both clauses produce zero rows total
MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) RETURN a, b;
```

Implementation: the planner injects an `EnsureNullRowIfEmpty`
operator after the first OPTIONAL pattern's scan when no prior
driver exists. The operator is a no-op when the scan produced
rows.

#### Required MATCH semantics (non-OPTIONAL Expand)

A required (non-OPTIONAL) pattern expansion that finds no matching
relationships drops the input row entirely, rather than emitting a phantom
partial row with the expansion's variables bound to `NULL`:

```cypher
-- If no edge exists from any Person to any Message:
MATCH (p:Person), (m:Message), (post:Post)
MATCH (m)-[:REPLY_OF]->(post)
RETURN p, m, post
-- Returns zero rows (no phantom [<p>, <m>, NULL] rows)
```

Use `OPTIONAL MATCH` to preserve rows with `NULL` bindings:

```cypher
MATCH (p:Person), (m:Message), (post:Post)
OPTIONAL MATCH (m)-[:REPLY_OF]->(post)
RETURN p, m, post
-- Returns rows with post = NULL if the optional edge doesn't exist
```

### UNWIND Clause

```cypher
-- List expansion
UNWIND [1, 2, 3] AS num
RETURN num * 2 AS doubled

-- With WHERE filtering
UNWIND $names AS name
WHERE name STARTS WITH 'A'
RETURN name

-- Bulk ingest: one CREATE per unwound row. The planner resolves property
-- expressions like `{id: id}` against the current row's UNWIND binding.
UNWIND range(0, 9) AS id
CREATE (n:Item {id: id})

-- UNWIND a list literal into a CREATE iteration
UNWIND ['alpha', 'beta', 'gamma'] AS name
CREATE (n:Listed {name: name})
```

### UNION / UNION ALL

```cypher
-- Union (distinct)
MATCH (n:Person) RETURN n
UNION
MATCH (n:Company) RETURN n

-- Union all (keep duplicates)
MATCH (n:Person) RETURN n
UNION ALL
MATCH (n:Company) RETURN n
```

### FOREACH Clause

```cypher
-- Iterate and update
MATCH (n:Person)
FOREACH (x IN [1, 2, 3] |
  CREATE (n)-[:TAG {value: x}]->(:Tag)
)
```

### Pattern Predicates

A relationship pattern is a boolean expression on its own — true when at least
one match exists — and may be written wherever an expression is legal, not only
after `NOT`:

```cypher
MATCH (n:Person) WHERE (n)-[:KNOWS]->()                RETURN n
MATCH (n:Person) WHERE NOT (n)-[:KNOWS]->()            RETURN n
MATCH (n:Person) WHERE exists((n)-[:KNOWS]->())        RETURN n
MATCH (n:Person) WHERE (n)-[:KNOWS]->() AND n.age > 30 RETURN n
MATCH (n:Person) RETURN n.name, (n)-[:KNOWS]->() AS knows_someone
```

All of these are equivalent to the `EXISTS { … }` subquery below and run through
the same evaluation.

A pattern predicate is recognised only when a relationship follows the first
node, so an ordinary parenthesized expression is never reinterpreted: `WHERE (a)`
is the variable `a`, and `WHERE (n.age) > 30` and `RETURN (1 + 2) = 3` mean what
they say.

### EXISTS Subqueries

```cypher
-- Existential pattern check (depth-first graph probe)
MATCH (n:Person)
WHERE EXISTS {
  MATCH (n)-[:KNOWS]->(:Person {city: 'NYC'})
}
RETURN n

-- Multi-hop pattern (walks chains via adjacency lists)
MATCH (n:Person)
WHERE EXISTS {
  MATCH (n)-[:MANAGES]->()-[:MANAGES]->(m:Person {seniority: 'executive'})
}
RETURN n

-- Inner WHERE filters candidates (subquery semantics)
MATCH (n:Person)
WHERE EXISTS {
  MATCH (n)-[r:KNOWS]->(friend)
  WHERE friend.age > 30
}
RETURN n

-- Correlated properties (outer-row binding visible inside)
MATCH (n:Person), (team:Team)
WHERE EXISTS {
  MATCH (n)-[:MEMBER_OF]->(t:Team {name: team.name})
}
RETURN n, team
```

**Supported pattern elements:**
- Node labels and multiple labels (`:L1:L2` = intersection; bare node matches all)
- Inline property constraints, including correlated ones like `(b {id: a.id})`
- Relationship types (single or union `:R1|R2|R3`), and directionality (→, ←, -)
- Relationship variables that can be bound and reused (reused rel obeys Cypher isomorphism — no rel satisfies two pattern hops)
- Variable-length relationships (e.g. `[:TYPE*1..3]`, `[*2]`, `[*0..1]`) with per-hop type and directionality filtering; zero-length matching when min is 0; relationship isomorphism across the variable-length segment; inline relationship properties applied per traversed edge. Bare `*` means `*1..` (openCypher default). The engine-wide 64-hop ceiling (see § Variable-length path bounded depth) applies to the max bound; a min bound above 64 can therefore never match
- Comma-separated pattern parts (each part anchors independently from outer variables)
- Anonymous nodes (no variable required)
- Inner WHERE clause (evaluated per candidate; `NULL` result excludes candidate, not `false`)

**Three-valued logic:**
- Outer variable bound to `NULL` → predicate returns `NULL` (filters as false under `WHERE`, composes correctly under `NOT EXISTS`)
- Inner `WHERE` returning `NULL` → candidate excluded (like `false`)
- `EXISTS { ... }` returns `NULL` only if ALL attempts to match hit a `NULL` correlated variable and no path succeeded

**Not yet supported:**
- Named relationship variables on a variable-length relationship inside EXISTS (e.g. `EXISTS { (a)-[r:T*]->(b) }`) — returns an explicit error (would require binding a `LIST<RELATIONSHIP>`)
- Quantified path patterns (QPP, e.g. `((a)-[:R]->(b))+`) inside EXISTS — returns an explicit "not implemented" error in all cases
- OPTIONAL MATCH inside EXISTS

### CASE Expressions

```cypher
-- Simple CASE
RETURN CASE n.status
  WHEN 'active' THEN 'Online'
  WHEN 'inactive' THEN 'Offline'
  ELSE 'Unknown'
END AS status_text

-- Generic CASE
RETURN CASE
  WHEN n.age < 18 THEN 'Minor'
  WHEN n.age < 65 THEN 'Adult'
  ELSE 'Senior'
END AS age_group

-- CASE (and list/pattern comprehensions) are supported inside WHERE too,
-- evaluated against each row — not only in RETURN/WITH projections.
MATCH (n) WHERE CASE WHEN n.x > 4 THEN true ELSE false END RETURN n
```

A `WHERE` clause is evaluated by carrying the parsed predicate AST through to
the filter operator (`Operator::Filter`/`OptionalFilter` `predicate_ast`) and
evaluating it directly against each row, the same evaluator `RETURN`/`WITH`
projections use — so `CASE`, list/pattern comprehensions, and other complex
expressions in a `WHERE` evaluate identically to their projected form.

### Comprehensions

```cypher
-- List comprehension
RETURN [x IN range(1,10) WHERE x % 2 = 0 | x * 2] AS evens

-- Pattern comprehension: enumerate matching paths from outer bindings via graph traversal
RETURN [(n)-[:KNOWS]->(f) | f.name] AS friends

-- Pattern comprehension with WHERE filter
RETURN [(n)-[:KNOWS]->(f) WHERE f.age > 30 | f.name] AS adult_friends

-- Path-shaped output via path binding (requires at least one relationship and | expr)
MATCH (start:Person {name: 'Alice'})
RETURN [p = (start)-[:KNOWS*1..3]->(person) | p] AS paths_to_others

-- Multi-hop enumeration with newly-bound variables
MATCH (n:Person)
RETURN [(n)-[:MANAGES]->(report)-[:MANAGES]->(sub) | {manager: n.name, report: report.name, subordinate: sub.name}] AS chains

-- Map projection (including nested pattern comprehensions)
RETURN n {.name, .age, friends: [(n)-[:KNOWS]->(f) | f.name]}
```

**Pattern comprehension semantics** (graph-traversing forms):

- **Enumeration:** a pattern comprehension `[(node-pattern) | expr]` enumerates all matching candidate paths from the anchored outer bindings via depth-first traversal. Each candidate is projected via the `| expr` transformation into the result list.
- **Newly-bound variables:** pattern variables bound inside the comprehension (e.g., `(f)` in `[(n)-[:KNOWS]->(f) | f.name]`) are available for projection and yield their matched values.
- **Path binding output:** when a path is bound via `[p = (n)-->() | expr]`, the `p` variable receives a path object `{nodes: [...], relationships: [...]}` in traversal order, and may be returned directly (`[p = ... | p]`) or transformed (`[p = ... | p.nodes]`). A path binding **requires** at least one relationship and a `| expr` projection (bare `[a = (b)]` parses as a normal list comprehension).
- **WHERE filtering:** an inner `WHERE` clause `[pattern WHERE filter | expr]` filters per candidate; `NULL` result excludes that candidate (subquery semantics, like EXISTS).
- **Correlated outer variables:** outer bindings are respected in pattern constraints, e.g. `[(n)-[:KNOWS]->(f {city: n.city}) | f.name]` filters matches by correlated city.
- **Anonymous nodes and mixed directions:** anonymous nodes (no variable), all relationship directions (→, ←, -), and variable-length segments are supported.
- **Variable-length segments and isomorphism:** variable-length relationships follow the same bounded-depth semantics as EXISTS: maximum 64 hops, relationship isomorphism (no relationship can satisfy two hops in the same path), per-hop type/directionality filtering, zero-length matching when min bound is 0. Each distinct trail through the segment produces one list element.
- **NULL outer variable:** a comprehension over a NULL outer anchor yields `[]` (mirrors `[x IN NULL | ...]`).
- **Result cap:** result materialization is guarded by the engine's intermediate-row capacity (explicit `OutOfMemory` error instead of unbounded growth).

**Unsupported and explicit errors:**
- **Multi-part patterns with path binding** (e.g., `[p = (a)--(), (b)--() | ...]`) raise an explicit error; path bindings require a single pattern anchor.
- **Quantified path patterns inside comprehensions** (e.g., `[(a)((b)-[:R]->(c)){1,3}(d) | ...]`) raise an explicit "not implemented" error.
- **Named relationship variables on variable-length segments** (e.g., `[p = (a)-[r:T*]->(b) | ...]`) raise an explicit "not implemented" error (same constraint as EXISTS — would require `LIST<RELATIONSHIP>` binding).

## Schema Management ✅ IMPLEMENTED

### Index Management

```cypher
-- Create index
CREATE INDEX ON :Person(email)

-- Create index if not exists
CREATE INDEX IF NOT EXISTS ON :Person(name)

-- Create or replace index
CREATE OR REPLACE INDEX ON :Person(age)

-- Create spatial index
CREATE SPATIAL INDEX ON :Location(coords)

-- Create vector index (native KNN search, dimension fixed at 128)
CREATE VECTOR INDEX ON :Person(embedding)

-- Create vector index if not exists
CREATE VECTOR INDEX IF NOT EXISTS ON :Person(embedding)

-- Create or replace vector index (resets the index)
CREATE OR REPLACE VECTOR INDEX ON :Person(embedding)

-- Drop index
DROP INDEX ON :Person(email)

-- Drop index if exists
DROP INDEX IF EXISTS ON :Person(name)

-- Drop vector index
DROP INDEX ON :Person(embedding)

-- Show all indexes
SHOW INDEXES
-- Returns: name, type, entityType, labelsOrTypes, properties

-- Show indexes and filter by label
SHOW INDEXES
-- Can be followed by a WHERE clause if needed in application code
```

**Vector Index Constraints (V1):**
- **Single global index**: Only one active vector index per database. Creating a second distinct index without `CREATE OR REPLACE` returns an error.
- **Fixed dimension**: Vector dimension is fixed at 128 floats per vector. All embeddings must have exactly 128 components.
- **Embedding supply**: Embeddings are supplied via query parameters or the data API — inline array literals in the CREATE DDL are NOT supported.

### Constraint Management

Both the legacy Cypher 4.x `ON … ASSERT` form and the Cypher 25 `FOR … REQUIRE`
form are supported. New queries should prefer `FOR … REQUIRE`; the legacy form is
kept for backward compatibility.

#### Legacy form (Cypher 4.x)

```cypher
-- Unique constraint
CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE

-- Exists constraint
CREATE CONSTRAINT ON (n:Person) ASSERT EXISTS(n.email)

-- Drop constraint
DROP CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE

-- Drop constraint if exists
DROP CONSTRAINT IF EXISTS ON (n:Person) ASSERT EXISTS(n.email)
```

#### Cypher 25 form — node scope

An optional constraint name may precede `IF NOT EXISTS`.

```cypher
CREATE CONSTRAINT FOR (n:Person) REQUIRE n.email IS UNIQUE
CREATE CONSTRAINT FOR (n:Person) REQUIRE n.email IS NOT NULL

-- Named, idempotent
CREATE CONSTRAINT person_email_unique IF NOT EXISTS
    FOR (n:Person) REQUIRE n.email IS UNIQUE

-- Composite node key
CREATE CONSTRAINT FOR (n:Person) REQUIRE (n.first, n.last) IS NODE KEY

-- Property type constraint
-- (INTEGER | FLOAT | STRING | BOOLEAN | BYTES | LIST | MAP)
CREATE CONSTRAINT FOR (n:Person) REQUIRE n.age IS :: INTEGER
```

#### Cypher 25 form — relationship scope

```cypher
CREATE CONSTRAINT FOR ()-[r:KNOWS]-() REQUIRE r.since IS NOT NULL
CREATE CONSTRAINT FOR ()-[r:KNOWS]-() REQUIRE r.weight IS :: FLOAT
```

A violated constraint raises `ERR_CONSTRAINT_VIOLATED`.

### Database Management

```cypher
-- Show databases
SHOW DATABASES

-- Create database
CREATE DATABASE mydb

-- Create database if not exists
CREATE DATABASE IF NOT EXISTS mydb

-- Use database
USE DATABASE mydb

-- Drop database
DROP DATABASE mydb

-- Drop database if exists
DROP DATABASE IF EXISTS mydb
```

### Function Management

```cypher
-- Create function signature
CREATE FUNCTION multiply(a: Integer, b: Integer) RETURNS Integer AS 'Multiply two integers'

-- Create function if not exists
CREATE FUNCTION IF NOT EXISTS add(a: Integer, b: Integer) RETURNS Integer

-- Show functions
SHOW FUNCTIONS

-- Drop function
DROP FUNCTION multiply

-- Drop function if exists
DROP FUNCTION IF EXISTS multiply
```

## Transaction Commands ✅ IMPLEMENTED

```cypher
-- Begin transaction
BEGIN

-- Commit transaction
COMMIT

-- Rollback transaction
ROLLBACK
```

## Query Analysis ✅ IMPLEMENTED

### EXPLAIN

```cypher
-- Query plan analysis
EXPLAIN MATCH (n:Person) RETURN n
```

### PROFILE

```cypher
-- Execution profiling
PROFILE MATCH (n:Person) RETURN n
```

### Query Hints ✅ IMPLEMENTED

```cypher
-- Force index usage. When a `PropertyIndex` handle is installed on
-- the planner (`QueryPlanner::with_property_index`), the planner
-- validates the hinted `(label, property)` pair against the
-- registered indexes and raises `ERR_USING_INDEX_NOT_FOUND` if the
-- pair has no matching property index. Without a handle the hint
-- is accepted silently — see `phase7_planner-using-index-hints`.
MATCH (n:Person)
USING INDEX n:Person(email)
WHERE n.email = 'alice@example.com'
RETURN n

-- Force label scan
MATCH (n:Person)
USING SCAN n:Person
WHERE n.age > 25
RETURN n

-- Force join strategy
MATCH (a)-[r]->(b)
USING JOIN ON r
RETURN a, b
```

## Query Management ✅ IMPLEMENTED

### SHOW QUERIES

List all currently executing queries across all connections.

```cypher
-- Show all running queries
SHOW QUERIES

-- Returns:
-- | queryId | query | database | user | startTime | elapsedMs | status |
-- |---------|-------|----------|------|-----------|-----------|--------|
-- | abc123  | MATCH | neo4j    | admin| 2025-01-15| 1234      | running|
```

### TERMINATE QUERY

Terminate a running query by its ID.

```cypher
-- Terminate a specific query
TERMINATE QUERY 'abc123'

-- Use with SHOW QUERIES to find long-running queries
SHOW QUERIES
-- Then terminate if needed
TERMINATE QUERY 'query-id-from-show-queries'
```

**Query Management Summary:**

| Command | Description |
|---------|-------------|
| `SHOW QUERIES` | List all running queries with metadata |
| `TERMINATE QUERY 'id'` | Cancel a running query by its ID |

## Data Import/Export ✅ IMPLEMENTED

### LOAD CSV

```cypher
-- Load CSV file
LOAD CSV FROM 'file:///path/to/data.csv' AS row
CREATE (n:Person {name: row[0], age: toInteger(row[1])})

-- With headers
LOAD CSV FROM 'file:///path/to/data.csv' WITH HEADERS AS row
CREATE (n:Person {name: row.name, age: toInteger(row.age)})
```

`LOAD CSV ... AS row` binds a fresh per-row variable, exactly like `UNWIND`.
The cost-based operator-reordering pass treats it as a per-row binder: it is
ordered before any `WHERE`/`Filter` that references the row variable, and
before a correlated seek it feeds (e.g. `LOAD CSV ... AS row MATCH (n:Label
{id: row.id})` with an index on `:Label(id)`), so a predicate on the CSV row —
or a `MATCH` seeded from it — is applied correctly rather than silently
dropped.

## Advanced Features ✅ IMPLEMENTED

### Subqueries

```cypher
-- CALL subquery (legacy form — every outer variable is visible)
CALL {
  MATCH (n:Person) RETURN n
}

-- CALL { } IN TRANSACTIONS OF N ROWS [REPORT STATUS AS var]
--                                    [ON ERROR CONTINUE|BREAK|FAIL|RETRY n]
-- Bulk-write recipe — drives N outer rows per batch through the inner
-- subquery. With REPORT STATUS, the operator emits one MAP-typed row
-- per batch under the named variable: { started, committed,
-- rowsProcessed, err }. ON ERROR controls failure recovery:
--   * FAIL (default): first error aborts the outer query.
--   * CONTINUE: rolls forward; failed batches surface as committed=false.
--   * BREAK: stops processing further batches.
--   * RETRY n: re-runs the failing batch up to n times before escalating.
UNWIND range(1, 100000) AS i
CALL { WITH i CREATE (:Audit {i: i}) }
IN TRANSACTIONS OF 1000 ROWS REPORT STATUS AS s ON ERROR CONTINUE
RETURN s.committed, s.rowsProcessed, s.err

-- CALL (vars) { … } — Cypher 25 scoped subquery. Only the listed outer
-- variables are visible inside the inner. `CALL () { … }` declares
-- a fully isolated inner scope.
MATCH (p:Person), (count_var)
CALL (p) { MATCH (q:Person) RETURN count(q) AS c }
RETURN p.name, c

-- COLLECT { … } subquery — folds every inner row into a LIST value.
--   * single-column inner → LIST<T>
--   * multi-column inner → LIST<MAP> keyed by RETURN column names
--   * aggregating inner → single-element list
--   * empty inner row stream → empty list (NOT NULL)
RETURN COLLECT { MATCH (p:Person) RETURN p.name } AS names
```

#### Concurrency cap

`CALL { … } IN CONCURRENT TRANSACTIONS` parses today, but the executor
refuses worker counts > 1 with `ERR_CALL_IN_TX_CONCURRENCY_UNSUPPORTED`
— the sharded-storage / per-worker MVCC isolation needed for multi-
worker subquery execution lands with the V2 distributed branch.

### Named Paths

```cypher
-- Path variable assignment
MATCH p = (a:Person)-[*]-(b:Person)
RETURN p, nodes(p), relationships(p), length(p)
```

**Where the assignment may appear.** A `variable =` prefix binds the
comma-separated pattern part that follows it, and may sit on **any** part of the
list — not only the first — in `MATCH`, `OPTIONAL MATCH` and `MERGE` alike:

```cypher
MATCH (), r = ()-[]-()                    -- assignment on the second part
MATCH p = (a)-[]-(b), q = (c)-[]-(d)      -- one per part
MATCH ()-[]-(), r = ()-[]-()              -- after a relationship part
MERGE p = (:A)-[:R]->(:B)                 -- MERGE takes one too
```

An identifier is read as a path assignment only when a single `=` follows it, so
a node variable (`MATCH (a), (b)`) and a comparison (`WHERE a = a`) are never
mistaken for one.

> **Binding is not yet complete.** The parser records each part's path variable
> and the semantic pass counts it as bound, but only a variable-length segment
> currently materialises a path value at execution time; over a fixed-length
> pattern the variable evaluates to `null`. Multi-part patterns are also
> independently incomplete — `MATCH ()-[]-(), ()-[]-() RETURN count(*)`
> under-counts — and that predates path assignment on later parts.

### Shortest Path Functions

```cypher
-- Shortest path
RETURN shortestPath((a:Person {name: 'Alice'})-[*]-(b:Person {name: 'Bob'}))

-- All shortest paths
RETURN allShortestPaths((a:Person)-[*]-(b:Person))
```

Both functions take their endpoints from variables bound by a preceding
`MATCH` (`MATCH (a), (b) RETURN shortestPath((a)-[...]->(b))`); a
`MATCH p = shortestPath(...)` assignment form is not supported. The
relationship pattern may name **multiple types** with `|` — `shortestPath((a)-[:R1|R2*..5]->(b))`
traverses every named type (OR-membership), and an unqualified `[*..n]`
matches every type. An unbounded `[*]` uses BFS with no depth cap.

## Built-in Functions ✅ IMPLEMENTED

### String Functions

```cypher
-- Basic string operations
RETURN toLower('HELLO') AS lower
RETURN toUpper('hello') AS upper
RETURN substring('Hello World', 0, 5) AS substr
RETURN trim('  hello  ') AS trimmed
RETURN ltrim('  hello') AS left_trimmed
RETURN rtrim('hello  ') AS right_trimmed
RETURN replace('hello world', 'world', 'nexus') AS replaced
RETURN split('a,b,c', ',') AS parts
RETURN length('hello') AS len

-- 2.5.0 additions
RETURN ascii('A') AS code            -- 65
RETURN chr(65) AS ch                 -- 'A'
RETURN lpad('7', 3, '0') AS padded   -- '007' (bounded allocation)
RETURN rpad('ab', 4, '.') AS padded  -- 'ab..' (bounded allocation)
RETURN normalize('café') AS nfc      -- NFC default; NFD/NFKC/NFKD via 2nd arg
RETURN randomUUID() AS uuid          -- v4 UUID string

**`lpad()` and `rpad()` bounded allocation:** Both functions cap the target length to **1,000,000 characters**, rejecting larger requests with a Cypher error before string allocation. Typical padding operations remain unaffected.
```

### Regex Functions ✅ IMPLEMENTED

```cypher
-- Test if pattern matches string
RETURN regexMatch('hello123world', '[0-9]+') AS hasNumbers        -- true
RETURN regexMatch('test@example.com', '^[^@]+@[^@]+$') AS isEmail  -- true

-- Replace first/all matches
RETURN regexReplace('a1b2c3', '[0-9]', 'X') AS first      -- 'aXb2c3'
RETURN regexReplaceAll('a1b2c3', '[0-9]', 'X') AS all     -- 'aXbXcX'

-- Normalize whitespace
RETURN regexReplaceAll('hello   world', '\\s+', ' ') AS normalized -- 'hello world'

-- Extract matches
RETURN regexExtract('hello123world456', '[0-9]+') AS first        -- '123'
RETURN regexExtractAll('a1b2c3', '[0-9]') AS all                   -- ['1', '2', '3']

-- Extract capture groups
RETURN regexExtractGroups('John Smith', '([A-Z][a-z]+) ([A-Z][a-z]+)') AS groups  -- ['John', 'Smith']
RETURN regexExtractGroups('2024-01-15', '([0-9]+)-([0-9]+)-([0-9]+)') AS date     -- ['2024', '01', '15']

-- Split by regex pattern
RETURN regexSplit('a1b2c3d', '[0-9]') AS parts            -- ['a', 'b', 'c', 'd']
RETURN regexSplit('hello   world  foo', '\\s+') AS words  -- ['hello', 'world', 'foo']
RETURN regexSplit('a,b;c,d;e', '[,;]') AS items           -- ['a', 'b', 'c', 'd', 'e']
```

**Available Regex Functions:**

| Function | Description |
|----------|-------------|
| `regexMatch(string, pattern)` | Returns `true` if pattern matches anywhere in string |
| `regexReplace(string, pattern, replacement)` | Replace first occurrence |
| `regexReplaceAll(string, pattern, replacement)` | Replace all occurrences |
| `regexExtract(string, pattern)` | Extract first match (or `null`) |
| `regexExtractAll(string, pattern)` | Extract all matches as array |
| `regexExtractGroups(string, pattern)` | Extract capture groups from first match |
| `regexSplit(string, pattern)` | Split string by regex pattern |

### Math Functions

```cypher
-- Basic math
RETURN abs(-5) AS absolute
RETURN ceil(4.3) AS ceiling
RETURN floor(4.7) AS floor
RETURN round(4.5) AS rounded
RETURN sqrt(16) AS square_root
RETURN pow(2, 3) AS power
RETURN log(8, 2) AS log_base2       -- 2.5.0: two-arg log(x, base)
RETURN isNaN(0.0/0.0) AS nan_check  -- 2.5.0
-- list: shuffle(list) — random permutation (2.5.0)
-- graph: elementId(n) → stable string "n:<id>" / "r:<id>" (2.5.0 format)

-- Trigonometric
RETURN sin(0) AS sine
RETURN cos(0) AS cosine
RETURN tan(0) AS tangent

-- Inverse trigonometric ✅ IMPLEMENTED
RETURN asin(0.5) AS arc_sine        -- Returns radians
RETURN acos(0.5) AS arc_cosine      -- Returns radians
RETURN atan(1) AS arc_tangent       -- Returns radians
RETURN atan2(1, 1) AS arc_tangent2  -- atan2(y, x) - returns radians

-- Exponential and logarithmic ✅ IMPLEMENTED
RETURN exp(1) AS e_to_power         -- e^x
RETURN log(10) AS natural_log       -- ln(x)
RETURN log10(100) AS log_base_10    -- log₁₀(x)

-- Angle conversion ✅ IMPLEMENTED
RETURN radians(180) AS rad          -- Degrees to radians (returns π)
RETURN degrees(3.14159) AS deg      -- Radians to degrees (returns ~180)

-- Mathematical constants ✅ IMPLEMENTED
RETURN pi() AS pi_value             -- Returns 3.141592653589793
RETURN e() AS euler_number          -- Returns 2.718281828459045
```

**Math Function Summary:**

| Function | Description |
|----------|-------------|
| `abs(x)` | Absolute value |
| `ceil(x)` | Round up to nearest integer |
| `floor(x)` | Round down to nearest integer |
| `round(x)` | Round to nearest integer |
| `sqrt(x)` | Square root |
| `pow(x, y)` | x raised to power y |
| `sin(x)` | Sine (x in radians) |
| `cos(x)` | Cosine (x in radians) |
| `tan(x)` | Tangent (x in radians) |
| `asin(x)` | Arc sine (returns radians) |
| `acos(x)` | Arc cosine (returns radians) |
| `atan(x)` | Arc tangent (returns radians) |
| `atan2(y, x)` | Arc tangent of y/x (returns radians) |
| `exp(x)` | e raised to power x |
| `log(x)` | Natural logarithm (ln) |
| `log10(x)` | Base-10 logarithm |
| `radians(x)` | Convert degrees to radians |
| `degrees(x)` | Convert radians to degrees |
| `pi()` | Returns π (3.14159...) |
| `e()` | Returns Euler's number (2.71828...) |
| `sign(x)` | Returns -1, 0, or 1 |
| `rand()` | Random number between 0 and 1 |

### Type Conversion

```cypher
-- Type conversions
RETURN toInteger('123') AS int_val
RETURN toFloat('3.14') AS float_val
RETURN toString(123) AS str_val
RETURN toBoolean('true') AS bool_val
RETURN toDate('2024-01-01') AS date_val
```

### Temporal Functions

```cypher
-- Current date/time functions
RETURN date() AS today
RETURN datetime() AS now
RETURN time() AS current_time
RETURN timestamp() AS unix_timestamp
RETURN localtime() AS local_time           -- ✅ IMPLEMENTED
RETURN localdatetime() AS local_datetime   -- ✅ IMPLEMENTED

-- Duration creation
RETURN duration({days: 7}) AS week
RETURN duration({years: 1, months: 6}) AS period
RETURN duration({hours: 2, minutes: 30}) AS time_duration

-- Temporal component extraction ✅ IMPLEMENTED
RETURN year(datetime('2025-01-15T10:30:00')) AS yr           -- 2025
RETURN month(datetime('2025-01-15T10:30:00')) AS mo          -- 1
RETURN day(datetime('2025-01-15T10:30:00')) AS dy            -- 15
RETURN hour(datetime('2025-01-15T10:30:00')) AS hr           -- 10
RETURN minute(datetime('2025-01-15T10:30:00')) AS min        -- 30
RETURN second(datetime('2025-01-15T10:30:00')) AS sec        -- 0
RETURN quarter(datetime('2025-04-15')) AS q                  -- 2
RETURN week(datetime('2025-01-15')) AS wk                    -- Week of year
RETURN dayOfWeek(datetime('2025-01-15')) AS dow              -- Day of week (1=Mon, 7=Sun)
RETURN dayOfYear(datetime('2025-01-15')) AS doy              -- Day of year (1-366)
RETURN millisecond(datetime('2025-01-15T10:30:00.123')) AS ms
RETURN microsecond(datetime('2025-01-15T10:30:00.123456')) AS us
RETURN nanosecond(datetime('2025-01-15T10:30:00.123456789')) AS ns

-- Create datetime from components
RETURN datetime({year: 2025, month: 1, day: 15}) AS dt
RETURN datetime({year: 2025, month: 1, day: 15, hour: 10, minute: 30, second: 0}) AS full_dt
RETURN date({year: 2025, month: 6, day: 15}) AS d
RETURN localtime({hour: 10, minute: 30, second: 0}) AS lt
RETURN localdatetime({year: 2025, month: 1, day: 15, hour: 10}) AS ldt
```

**Sub-second components are additive.** `millisecond`, `microsecond` and
`nanosecond` each occupy their own decimal slot of the nine-digit fraction and
sum into it — `millisecond * 1_000_000 + microsecond * 1_000 + nanosecond`. A
component given on its own spans every digit below it, so `{microsecond: 645876}`
is `.645876`, not `.000645876`. This applies to all four instant constructors
(`localtime`, `time`, `localdatetime`, `datetime`); `duration({...})` uses its own
plural keys and is unrelated.

```cypher
RETURN localtime({hour: 12, minute: 31, second: 14, millisecond: 123,
                  microsecond: 456, nanosecond: 789})     -- 12:31:14.123456789
RETURN localtime({hour: 12, minute: 31, second: 14, microsecond: 645876})
                                                          -- 12:31:14.645876
RETURN localtime({hour: 12, minute: 31, second: 14, millisecond: 645,
                  nanosecond: 2})                         -- 12:31:14.645000002
RETURN localtime({hour: 12, minute: 31, second: 14})      -- 12:31:14, no fraction
```

A component's permitted range depends on which coarser components are present,
because a coarser neighbour has already claimed the leading digits: `millisecond`
is always `[0, 999]`; `microsecond` is `[0, 999]` when `millisecond` is given and
`[0, 999999]` otherwise; `nanosecond` is `[0, 999]` when `microsecond` is given,
`[0, 999999]` when only `millisecond` is, and `[0, 999999999]` when it stands
alone. Out of range, negative, or fractional is an `InvalidArgumentValue` error
naming the key and its bound — never a silent fold into a different quantity.

The rendered fraction carries only as many digits as the value needs, with
trailing zeros dropped, which is why the three examples above render 9, 6 and 9
digits respectively and a whole second renders no fraction at all.

**Temporal Component Functions Summary:**

| Function | Description |
|----------|-------------|
| `year(temporal)` | Extract year component |
| `month(temporal)` | Extract month (1-12) |
| `day(temporal)` | Extract day of month (1-31) |
| `hour(temporal)` | Extract hour (0-23) |
| `minute(temporal)` | Extract minute (0-59) |
| `second(temporal)` | Extract second (0-59) |
| `quarter(temporal)` | Extract quarter (1-4) |
| `week(temporal)` | Extract ISO week of year (1-53) |
| `dayOfWeek(temporal)` | Day of week (1=Monday to 7=Sunday) |
| `dayOfYear(temporal)` | Day of year (1-366) |
| `millisecond(temporal)` | Extract milliseconds (0-999) |
| `microsecond(temporal)` | Extract microseconds (0-999999) |
| `nanosecond(temporal)` | Extract nanoseconds (0-999999999) |

### Temporal Arithmetic ✅ IMPLEMENTED

```cypher
-- Datetime + Duration
RETURN datetime('2025-01-15T10:30:00') + duration({days: 5}) AS future
RETURN datetime('2025-01-15T10:30:00') + duration({months: 2}) AS later
RETURN datetime('2025-01-15T10:30:00') + duration({years: 1}) AS next_year

-- Datetime - Duration
RETURN datetime('2025-01-15T10:30:00') - duration({days: 5}) AS past
RETURN datetime('2025-03-15T10:30:00') - duration({months: 2}) AS earlier

-- Date + Duration
RETURN date('2025-01-15') + duration({days: 10}) AS later_date

-- Datetime - Datetime (returns duration)
RETURN datetime('2025-01-20T10:30:00') - datetime('2025-01-15T10:30:00') AS diff

-- Duration + Duration
RETURN duration({days: 3}) + duration({days: 2}) AS combined

-- Duration - Duration
RETURN duration({days: 5}) - duration({days: 2}) AS difference
```

### Duration Functions ✅ IMPLEMENTED

```cypher
-- Duration between datetimes
RETURN duration.between(datetime('2025-01-15'), datetime('2025-01-20')) AS diff

-- Get duration in specific units
RETURN duration.inMonths(datetime('2025-01-01'), datetime('2025-04-01')) AS months
RETURN duration.inDays(datetime('2025-01-01'), datetime('2025-01-15')) AS days
RETURN duration.inSeconds(datetime('2025-01-01T00:00:00'), datetime('2025-01-01T01:00:00')) AS seconds
```

### List Functions

```cypher
-- List operations
RETURN size([1, 2, 3]) AS list_size
RETURN head([1, 2, 3]) AS first
RETURN tail([1, 2, 3]) AS rest
RETURN last([1, 2, 3]) AS last_item
RETURN range(1, 10) AS numbers
RETURN range(0, 1000000, 2) AS step_range  -- bounded allocation
RETURN reverse([1, 2, 3]) AS reversed
RETURN reduce(acc = 0, x IN [1, 2, 3] | acc + x) AS sum
RETURN [x IN [1, 2, 3] | x * 2] AS doubled
```

**`range()` bounded allocation:** `range(start, end [, step])` rejects queries whose element count exceeds **2,000,000** with a Cypher error. Checked arithmetic prevents wraparound on huge step values, so `range(0, 9223372036854775807, 3)` returns an error instead of looping. Well-formed ranges within 2M elements are unaffected.

### Path Functions

```cypher
-- Path operations
MATCH p = (a)-[*]-(b)
RETURN nodes(p) AS path_nodes
RETURN relationships(p) AS path_rels
RETURN length(p) AS path_length
```

**Path value representation.** A path binds as the alternating node/relationship
sequence `[n0, r0, n1, r1, …, nN]`. `nodes()` keeps the node elements,
`relationships()` keeps the relationship elements, and `length()` is the
relationship count — so a zero-length path (`(a)-[*0..1]->(b)` matching `a`
against itself) has one node, no relationships and length `0`.

**Null propagation.** `nodes(null)`, `relationships(null)` and `length(null)`
all return `null`, not an empty list or `0`. This covers the
`OPTIONAL MATCH p = …` no-match case, where `p` itself is `null`: an empty list
would wrongly read as "a real path that happens to be empty".

**`length()` argument type.** `length()` applies to paths, and to strings and
lists as a size. Applying it to a **node or relationship** is a compile-time
error (`SyntaxError` / `InvalidArgumentType`), raised by the semantic-validation
pass before execution rather than answered with `0`:

```cypher
MATCH (n) RETURN length(n)        -- error: InvalidArgumentType
MATCH ()-[r]->() RETURN length(r) -- error: InvalidArgumentType
RETURN length('hello')            -- 5 (character count)
```

The check judges a directly-named pattern variable, which is where the type is
statically known; a property access, function result, parameter or `WITH` alias
is left to runtime, so the pass never rejects a query that might be valid.

> **Known divergence:** `length(string)` counts characters while the sibling
> `size(string)` counts UTF-8 bytes, so the two disagree on non-ASCII input.
> `size()` is the one that is wrong; it is not corrected here.

### Expression Forms and Parse Strictness

**A query that cannot be read in full is rejected.** The parser used to stop at
the first expression form it could not continue, keep what it had, and drop the
rest of the projection list — and any clause after it — while reporting success.
That is gone: leftover input is an error naming what was left unread. A single
trailing `;` is accepted as a statement terminator.

```cypher
MATCH (a)(b) RETURN a     -- error: juxtaposed node patterns are not Cypher
RETURN 1 AS a GROUP BY a  -- error: Cypher has no GROUP BY (grouping is implicit)
```

**`IS NULL` / `IS NOT NULL` bind looser than a comparison**, so they apply to the
whole comparison:

```cypher
RETURN false = true IS NULL      -- (false = true) IS NULL  -> false
RETURN false = (true IS NULL)    -- the other reading       -> true
```

**Property access on a computed base** — a parenthesised expression or a call
result — is supported, and composes with a call's own indexing:

```cypher
WITH [123, {existing: 42}] AS list RETURN (list[1]).existing   -- 42
MATCH ()-[r]->() RETURN startNode(r).id, endNode(r).id
WITH {a: {b: 7}} AS m RETURN (m.a).b                           -- 7
```

A property read off a non-container or `NULL` base yields `NULL` rather than an
error, matching `null.prop`.

**Comparison chaining** works: `a < b < c` means `a < b AND b < c`, and extends
to further links.

> **Known gaps, both of which now fail loudly rather than truncating:** a slice on
> a bare variable (`l[1..3]`) is rejected by the index parser, and indexing a
> parenthesised expression (`(l)[1]`) is not supported — the parenthesised form
> accepts `.property` suffixes only. Sharing one index/slice loop across every
> postfix position is the follow-up.

### Predicate Functions

```cypher
-- Predicates
RETURN all(x IN [1, 2, 3] WHERE x > 0) AS all_positive
RETURN any(x IN [1, 2, 3] WHERE x > 2) AS any_greater
RETURN none(x IN [1, 2, 3] WHERE x < 0) AS none_negative
RETURN single(x IN [1, 2, 3] WHERE x = 2) AS single_match
```

**The bound variable is local to the quantifier.** `x` above is introduced by the
quantifier and bound per list item; it is not a reference to the enclosing scope,
so it neither needs a preceding clause to define it nor contributes a query-level
filter. That makes the four predicates valid in a **standalone** projection — a
bare `RETURN` or `WITH` with no reading clause before it, which is how they are
most often written — and equally valid nested inside a larger expression:

```cypher
RETURN all(x IN [1,2] WHERE x > 0) AND any(y IN [3] WHERE y > 2) AS both
WITH all(x IN [1,2] WHERE x > 0) AS ok RETURN ok
```

Over an **empty list** the vacuous-quantification rules apply: `all` and `none`
return `true`, `any` and `single` return `false`.

`filter(x IN list WHERE pred)` (the deprecated form) and the list comprehension
`[x IN list WHERE pred]` scope their variable the same way.

> **Known gap:** `extract(x IN list | expr)` and `reduce(acc = init, x IN list |
> expr)` are not parseable in that syntax — both raise a syntax error. The
> evaluator implements them; only the surface form is missing.

### Additional Aggregations

```cypher
-- Advanced aggregations
RETURN percentileDisc(n.age, 0.5) AS median
RETURN percentileCont(n.age, 0.5) AS median_cont
RETURN stDev(n.age) AS std_dev
RETURN stDevP(n.age) AS std_dev_pop
```

## Geospatial Features ✅ IMPLEMENTED

### Point Data Type

```cypher
-- Create 2D Cartesian point
RETURN point({x: 1, y: 2}) AS p

-- Create 3D Cartesian point
RETURN point({x: 1, y: 2, z: 3}) AS p3d

-- Create WGS84 geographic point (longitude, latitude)
RETURN point({x: -122.4194, y: 37.7749, crs: 'wgs-84'}) AS location

-- Alternative: Use latitude/longitude directly
RETURN point({longitude: -122.4194, latitude: 37.7749}) AS location
```

### Point Property Accessors ✅ IMPLEMENTED

```cypher
-- Access point components
WITH point({x: -122.4194, y: 37.7749, z: 100, crs: 'wgs-84'}) AS p
RETURN p.x AS x,                    -- X coordinate (longitude for WGS84)
       p.y AS y,                    -- Y coordinate (latitude for WGS84)
       p.z AS z,                    -- Z coordinate (if 3D point)
       p.latitude AS lat,           -- Alias for y (WGS84)
       p.longitude AS lon,          -- Alias for x (WGS84)
       p.crs AS coordinate_system   -- 'cartesian' or 'wgs-84'
```

**Point Property Summary:**

| Property | Description |
|----------|-------------|
| `point.x` | X coordinate (or longitude for WGS84) |
| `point.y` | Y coordinate (or latitude for WGS84) |
| `point.z` | Z coordinate (optional, for 3D points) |
| `point.latitude` | Alias for y (WGS84 points) |
| `point.longitude` | Alias for x (WGS84 points) |
| `point.crs` | Coordinate reference system |

### Distance Functions

```cypher
-- Calculate distance between two points
WITH point({x: 0, y: 0}) AS p1, point({x: 3, y: 4}) AS p2
RETURN distance(p1, p2) AS dist  -- Returns 5.0 (Euclidean distance)

-- Calculate distance to stored locations
MATCH (l:Location)
RETURN l.name, distance(l.coords, point({x: -122.4194, y: 37.7749, crs: 'wgs-84'})) AS dist
ORDER BY dist LIMIT 5
```

### Geospatial Procedures

```cypher
-- Find nodes within a bounding box
CALL spatial.withinBBox('Location', 'coords',
  point({x: -122.5, y: 37.7}),  -- min corner
  point({x: -122.3, y: 37.8})   -- max corner
)
YIELD node
RETURN node.name, node.coords

-- Find nodes within distance (radius search)
CALL spatial.withinDistance('Location', 'coords',
  point({x: -122.4194, y: 37.7749, crs: 'wgs-84'}),
  1000.0  -- distance in meters for WGS84
)
YIELD node
RETURN node.name, node.coords
```

## Graph Algorithms ✅ IMPLEMENTED

### Pathfinding

```cypher
-- Shortest path
MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
RETURN shortestPath((a)-[*]-(b)) AS path

-- All shortest paths
MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
RETURN allShortestPaths((a)-[*]-(b)) AS paths
```

### Centrality

```cypher
-- PageRank
CALL algorithms.pageRank('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
ORDER BY score DESC LIMIT 10
```

### Community Detection

```cypher
-- Label propagation
CALL algorithms.labelPropagation('Person', 'KNOWS')
YIELD node, community
RETURN community, collect(node.name) AS members
```

## GDS Procedure Wrappers ✅ IMPLEMENTED

Nexus provides Neo4j GDS-compatible procedure wrappers for graph algorithms.

### Centrality Algorithms

```cypher
-- PageRank centrality (standard)
CALL gds.centrality.pagerank('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
ORDER BY score DESC

-- PageRank with custom parameters
CALL gds.centrality.pagerank('Person', 'KNOWS', {
  dampingFactor: 0.85,      -- Probability of following a link (default: 0.85)
  maxIterations: 100,       -- Maximum iterations (default: 100)
  tolerance: 0.0001         -- Convergence tolerance (default: 0.0001)
})
YIELD node, score
RETURN node.name, score

-- Weighted PageRank (uses edge weights for contribution) ✅ NEW
CALL gds.centrality.pagerank.weighted('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score

-- Betweenness centrality
CALL gds.centrality.betweenness('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score

-- Closeness centrality
CALL gds.centrality.closeness('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score

-- Degree centrality
CALL gds.centrality.degree('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score

-- Eigenvector centrality
CALL gds.centrality.eigenvector('Person', 'KNOWS')
YIELD node, score
RETURN node.name, score
```

**PageRank Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `dampingFactor` | Float | 0.85 | Probability of following a link vs teleporting |
| `maxIterations` | Integer | 100 | Maximum number of iterations |
| `tolerance` | Float | 0.0001 | Convergence threshold |

**PageRank Variants:**

| Procedure | Description |
|-----------|-------------|
| `gds.centrality.pagerank` | Standard PageRank with equal edge weights |
| `gds.centrality.pagerank.weighted` | PageRank using edge weights for contribution distribution |

**Performance Notes:**
- For graphs with >1000 nodes, parallel processing is automatically enabled
- Weighted PageRank distributes rank proportionally to edge weights
- Higher edge weights mean more PageRank flows through that connection

### Pathfinding Algorithms

```cypher
-- Dijkstra shortest path
CALL gds.shortestPath.dijkstra('Person', 'KNOWS', $sourceId, $targetId)
YIELD path, cost
RETURN path, cost

-- A* shortest path (with heuristic)
CALL gds.shortestPath.astar('Person', 'KNOWS', $sourceId, $targetId)
YIELD path, cost
RETURN path, cost

-- Yen's K shortest paths
CALL gds.shortestPath.yens('Person', 'KNOWS', $sourceId, $targetId, 5)
YIELD path, cost
RETURN path, cost
```

### Community Detection

```cypher
-- Louvain community detection (modularity optimization)
CALL gds.community.louvain('Person', 'KNOWS')
YIELD node, community
RETURN community, collect(node.name) AS members
ORDER BY size(collect(node.name)) DESC

-- Label propagation (fast, semi-supervised)
CALL gds.community.labelPropagation('Person', 'KNOWS')
YIELD node, community
RETURN community, collect(node.name) AS members

-- Weakly connected components
CALL gds.components.weaklyConnected('Person', 'KNOWS')
YIELD node, componentId
RETURN componentId, count(*) AS size

-- Strongly connected components (for directed graphs)
CALL gds.components.stronglyConnected('Person', 'KNOWS')
YIELD node, componentId
RETURN componentId, count(*) AS size
```

**Community Detection Algorithms:**

| Algorithm | Best For | Complexity | Description |
|-----------|----------|------------|-------------|
| Louvain | Large graphs | O(n log n) | Modularity-based, hierarchical |
| Label Propagation | Very large graphs | O(m) | Fast, near-linear time |
| WCC | Any graph | O(n + m) | Finds connected subgraphs |
| SCC | Directed graphs | O(n + m) | Finds strongly connected subgraphs |

**Notes:**
- Louvain produces high-quality communities but may be slower
- Label Propagation is faster but results can vary between runs
- WCC/SCC are deterministic and fast

### Graph Structure Analysis

```cypher
-- Triangle counting (per node)
CALL gds.triangleCount('Person', 'KNOWS')
YIELD node, triangles
RETURN node.name, triangles
ORDER BY triangles DESC

-- Local clustering coefficient (how clustered neighbors are)
CALL gds.localClusteringCoefficient('Person', 'KNOWS')
YIELD node, coefficient
RETURN node.name, coefficient
ORDER BY coefficient DESC

-- Global clustering coefficient (overall graph clustering)
CALL gds.globalClusteringCoefficient('Person', 'KNOWS')
YIELD coefficient
RETURN coefficient
```

**Graph Structure Metrics:**

| Metric | Returns | Description |
|--------|---------|-------------|
| Triangle Count | Per-node count | Number of triangles each node participates in |
| Local Clustering | Per-node coefficient [0,1] | Probability that neighbors are connected |
| Global Clustering | Single coefficient [0,1] | Overall graph transitivity |

**Use Cases:**
- Triangle count: Identify highly interconnected nodes
- Local clustering: Find tightly-knit groups
- Global clustering: Measure overall network cohesion

### GDS Procedure Summary

| Procedure | Category | Description |
|-----------|----------|-------------|
| `gds.centrality.pagerank` | Centrality | Standard PageRank algorithm |
| `gds.centrality.pagerank.weighted` | Centrality | Weighted PageRank (edge weights) |
| `gds.centrality.betweenness` | Centrality | Betweenness centrality |
| `gds.centrality.closeness` | Centrality | Closeness centrality |
| `gds.centrality.degree` | Centrality | Degree centrality |
| `gds.centrality.eigenvector` | Centrality | Eigenvector centrality |
| `gds.shortestPath.dijkstra` | Pathfinding | Dijkstra's algorithm |
| `gds.shortestPath.astar` | Pathfinding | A* with heuristic |
| `gds.shortestPath.bellmanFord` | Pathfinding | Bellman-Ford (negative weights) |
| `gds.shortestPath.yens` | Pathfinding | K shortest paths |
| `gds.community.louvain` | Community | Louvain modularity optimization |
| `gds.community.labelPropagation` | Community | Fast label propagation |
| `gds.components.weaklyConnected` | Community | Weakly connected components |
| `gds.components.stronglyConnected` | Community | Strongly connected components |
| `gds.triangleCount` | Structure | Triangle counting per node |
| `gds.localClusteringCoefficient` | Structure | Per-node clustering coefficient |
| `gds.globalClusteringCoefficient` | Structure | Graph-wide clustering coefficient |
| `gds.similarity.jaccard` | Similarity | Jaccard similarity score |
| `gds.similarity.cosine` | Similarity | Cosine similarity score |

**Total: 19 GDS procedures implemented**

### Schema Introspection Procedures

Nexus provides a suite of read-only schema procedures for inspecting catalog metadata, index/constraint definitions, and property keys. These procedures honor the per-request `database` field on `/cypher` — a `CALL db.labels()` on `database:"alpha"` queries the `alpha` database's catalog, not the default.

**YIELD projection ordering (ORDER BY, SKIP, LIMIT).** Procedure YIELD projections support ORDER BY, SKIP, and LIMIT clauses in the standard openCypher order. For example, `CALL db.labels() YIELD label RETURN label ORDER BY label SKIP 1 LIMIT 10` will sort the labels alphabetically, skip the first result, and return the next 10 — all clauses now apply correctly to pattern-less YIELD outputs.

| Procedure | Returns | Description |
|-----------|---------|-------------|
| `db.labels()` | List of label names | All node labels currently in use (single column: `label`). |
| `db.propertyKeys()` | List of property key names | All properties observed on writes (single column: `propertyKey`). Keys are registered at every property write via engine CRUD, executor CREATE, SET/MERGE on relationships, and bulk loaders — not only at DDL. |
| `db.relationshipTypes()` | List of relationship type names | All relationship types currently in use (single column: `relationshipType`). |
| `db.schema()` | `nodes` array, `relationships` array | Combined schema: nested JSON objects with `name` fields, useful for schema inspection in a single query. |
| `db.info()` | id, name, creationDate | Database info: `id` (usually `"db-1"`), `name` (database name), `creationDate` (ISO 8601 timestamp). Single row. |
| `db.indexes()` | Index metadata | All indexes (label bitmaps, composite B-tree, KNN, full-text, R-tree): columns `id, name, state, populationPercent, uniqueness, type, entityType, labelsOrTypes, properties, indexProvider, options`. Filterable via `YIELD` clause. |
| `db.indexDetails(indexName)` | Single index metadata | Same columns as `db.indexes()`, filtered to a specific index by name (useful for detailed inspection of one index). |
| `db.constraints()` | Constraint metadata | All constraints (UNIQUENESS, NODE_PROPERTY_EXISTENCE): columns `id, name, type, entityType, labelsOrTypes, properties, ownedIndex`. |

**Examples:**

```cypher
-- List all labels in the current database
CALL db.labels() YIELD label
RETURN label
ORDER BY label

-- List all property keys (includes keys from live writes)
CALL db.propertyKeys() YIELD propertyKey
RETURN propertyKey

-- Get full schema in one call
CALL db.schema() YIELD nodes, relationships
RETURN nodes, relationships

-- Inspect a specific index
CALL db.indexDetails('index_label_Person') YIELD name, type, properties
RETURN name, type, properties

-- Query a different database's schema
POST /cypher
{
  "query": "CALL db.labels() YIELD label RETURN label",
  "database": "alpha"
}
```

**Additional System Procedures:**

| Procedure | Returns | Description |
|-----------|---------|-------------|
| `dbms.components()` | Component info | Nexus components/versions. |
| `dbms.procedures()` | Procedure list | All built-in procedures with signatures. |
| `dbms.functions()` | Function list | All built-in functions. |
| `dbms.info()` | System info | Nexus system information. |
| `dbms.listConfig(pattern?)` | Config parameters | Configuration keys matching an optional pattern. |
| `dbms.showCurrentUser()` | User info | Current authenticated user (if auth enabled). |
| `db.index.fulltext.queryNodes(indexName, query, limit?)` | Full-text results | Query a full-text node index by name. |
| `db.index.fulltext.queryRelationships(indexName, query, limit?)` | Full-text results | Query a full-text relationship index by name. |
| `db.index.fulltext.listAvailableAnalyzers()` | Analyzer list | Available Tantivy analyzers for full-text indexes. |
| `spatial.nearest(label, property, point, limit?)` | Spatial results | K-nearest spatial neighbors from R-tree index. |

## Unsupported Features (Out of Scope)

### Not Currently Implemented

```cypher
-- Complex subqueries with multiple statements (future)
-- CALL {
--   MATCH (n:Person) RETURN n
--   UNION
--   MATCH (n:Company) RETURN n
-- }

-- Full-text search syntax (use procedures instead)
-- MATCH (n:Person) WHERE n.bio CONTAINS TEXT 'engineer'
```

## Query Optimization Hints

### Planner Hints (V1, optional)

```cypher
-- Force index usage
MATCH (n:Person)
USING INDEX n:Person(email)
WHERE n.email = 'alice@example.com'
RETURN n

-- Force label scan
MATCH (n:Person)
USING SCAN n:Person
WHERE n.age > 25
RETURN n
```

## Error Handling

### Syntax Errors

```cypher
-- Missing RETURN
MATCH (n:Person)
-- Error: Expected RETURN clause

-- Invalid pattern
MATCH (n:Person)-->(m)
-- Error: Relationship must have type

-- Type mismatch
WHERE n.age = 'thirty'
-- Error: Type mismatch: expected Integer, got String
```

### Semantic Validation

A static semantic-analysis pass runs after parsing and before planning. It
rejects queries that parse cleanly but violate openCypher semantic rules
(which would otherwise execute and silently return wrong or empty rows),
raising a `SyntaxError` that carries the openCypher detail token. The pass is
conservative — it never rejects a currently-valid query — and skips queries
containing constructs whose scoping it does not yet fully model (`UNION`,
`CALL {…}` subqueries, `CALL` procedures, `LOAD CSV`).

| Rejected query | Detail token |
|---|---|
| `MATCH (a) RETURN b` (variable bound nowhere) | `UndefinedVariable` |
| `MATCH (a) MATCH ()-[a]-()` (node reused as relationship) | `VariableTypeConflict` |
| `MATCH (a) CREATE (a {x: 1})` (re-declare a bound node with structure) | `VariableAlreadyBound` |
| `RETURN count(count(*))` (aggregate inside aggregate) | `NestedAggregation` |
| `MATCH (a) WHERE count(a) > 1 RETURN a` (aggregate in WHERE) | `InvalidAggregation` |
| `RETURN n SKIP -1` (negative integer literal) | `NegativeIntegerArgument` |
| `RETURN n SKIP n.count` (SKIP/LIMIT depends on a variable) | `NonConstantExpression` |
| `RETURN 1 AS a, 2 AS a` (duplicate projection alias) | `ColumnNameConflict` |
| `MATCH (a)-[r]->()-[r]->(a)` (relationship variable reused in one pattern) | `RelationshipUniquenessViolation` |

**The pass applies to every transport and every entry point.** It runs at the two
places an executed query must pass through: the engine's shared AST body (which
covers both the parse-the-text entry point and the pre-parsed-AST one the binary
RPC transport uses) and the executor's own parse (which covers the pure-read
requests both transports deliberately route around the engine lock, straight onto
a cloned executor). A semantically invalid query is therefore rejected with the
same detail token whichever SDK or transport sent it — which was not true before:
validation used to hang off the single entry point that parses query text, so
whether a client saw an error depended on the query's shape and its transport.
`api::cypher::semantic_validation_parity` pins both surfaces against one server
so they cannot drift again.

Not yet detected (deferred refinements): `AmbiguousAggregationExpression`
(implicit-grouping analysis), `UNION` column-structure checks
(`DifferentColumnsInUnion`, `InvalidClauseComposition`), the bare
`CREATE (a)` re-declaration and `MATCH`/`MERGE` re-binding forms, and
`NoVariablesInScope` (unreachable while the parser rejects `RETURN *`/`WITH *`
outright).

### Runtime Errors

```cypher
-- Node not found (returns empty result, not error)
MATCH (n:Person {name: 'NonExistent'})
RETURN n
-- Result: (empty)

-- Division by zero (V1)
-- RETURN 1 / 0
-- Error: Division by zero

-- Property access on null
RETURN null.name
-- Error: Cannot access property on null
```

### Serialization-dependent Runtime Errors (`phase2_propagate-serde-errors`)

Operators that build canonical row/group keys via JSON serialisation
**now propagate the error** instead of silently coercing the failing
row into an empty-string bucket. This matters in practice when a
property holds a non-finite float (`NaN`, `+Infinity`, `-Infinity`) or
any other value the JSON data model cannot represent.

| Clause | Error message prefix | Failure mode before phase2 |
|--------|----------------------|----------------------------|
| `GROUP BY` | `GROUP BY key serialization failed (…)` | All failing rows collapsed into one bogus group with key `""`. |
| `DISTINCT` | `DISTINCT key serialization failed (…)` | Unrelated failing rows silently deduplicated together. |
| `UNION` (not `UNION ALL`) | `UNION dedup key serialization failed (…)` | Same as DISTINCT, across UNION branches. |

Each failure bumps the Prometheus counter
`nexus_executor_serde_fallback_total{site="<op>"}` (label values:
`aggregate_group_key`, `distinct_key`, `union_dedup_key`). Operators
wishing to sidestep the error can use `UNION ALL` instead of `UNION`,
strip the offending property before grouping, or coerce non-finite
floats to `null`.

A secondary fallback path at `eval/helpers.rs::update_result_set_from_rows`
keeps its previous dedup-best-effort behaviour but now logs a
`tracing::warn!` and bumps
`nexus_executor_serde_fallback_total{site="helper_row_dedup_key"}` so
the degradation is observable. The key is degraded to the Rust `Debug`
representation of the failing value (distinct values still produce
distinct keys), never to the empty string.

Cache-warming failures inside the executor hot path
(`Executor::execute` lazy warmup) are similarly logged and counted
under `site="warm_cache_lazy"` instead of being silently dropped.

## Plan cache (`phase8_query-plan-cache`)

Every query goes through a process-wide LRU plan cache before
hitting the optimizer. Repeated parameterised queries — the
typical RAG / hot-endpoint shape — hit the cache and skip the
parse + plan cost on every call after the first.

| Knob | Default | Effect |
|---|---|---|
| `NEXUS_PLAN_CACHE_ENTRIES` | `1024` | LRU bound (number of cached plans). |
| `NEXUS_PLAN_CACHE_DISABLE` | unset | When `1` / `true` / `yes`, every lookup misses and every insert is a no-op. Counters keep ticking so an operator who flips the knob mid-flight sees the hit-rate drop. |

**Key**: `xxh3_64` of the canonicalised query text. Canonicaliser
strips comments, collapses whitespace runs, and trims leading /
trailing whitespace — see `crates/nexus-core/src/executor/planner/cache.rs`
for the per-rule documentation. Parameter *values* are not part
of the key; parameter *names* are (different `$x` / `$y` produce
different plans).

**Invalidation**: a `planner_generation: AtomicU64` bumps on
schema-change hooks (CREATE / DROP INDEX, label / type / key
registry mutations); cached entries stamp their generation on
populate and surface as misses on the next lookup if the
generation moved. Full flush via `PlanCache::clear()` is the
operator-emergency escape.

**Stats**: `PlanCache::stats()` returns
`{ hits, misses, evictions, size, capacity, enabled, generation }`.
Counters are monotonic across the process lifetime — `clear()`
drops entries but does not reset hit / miss totals so trend lines
stay clean.

## Performance Characteristics

| Query Pattern | Complexity | Notes |
|---------------|------------|-------|
| `MATCH (n:Label)` | O(\|V_label\|) | Label bitmap scan |
| `MATCH (n:Label {id: $id})` | O(1) | With index on `id` |
| `MATCH (n)-[r]->(m)` | O(\|V\| × avg_degree) | Expand neighbors |
| `MATCH (n)-[:TYPE*1..3]->(m)` | O(\|V\| × avg_degree^3) | Variable-length path |
| `ORDER BY ... LIMIT k` | O(n log k) | Top-K heap |
| `COUNT(*)` | O(n) | Full scan or index count |
| `vector.knn(...)` | O(log n) | HNSW logarithmic |

## Parser Grammar (EBNF)

```ebnf
Query ::= MatchClause WhereClause? ReturnClause OrderByClause? LimitClause? SkipClause?
        | CallClause YieldClause? WhereClause? ReturnClause?

MatchClause ::= 'MATCH' Pattern (',' Pattern)*

Pattern ::= NodePattern (RelationshipPattern NodePattern)*

NodePattern ::= '(' Variable? LabelExpression? PropertyMap? ')'

LabelExpression ::= (':' Identifier)+

RelationshipPattern ::= 
    '-[' Variable? ':' Type PropertyMap? ']->' |
    '<-[' Variable? ':' Type PropertyMap? ']-' |
    '-[' Variable? ':' Type PropertyMap? ']-'

PropertyMap ::= '{' (Property (',' Property)*)? '}'

Property ::= Identifier ':' Expression

WhereClause ::= 'WHERE' Expression

ReturnClause ::= 'RETURN' ('DISTINCT')? ReturnItem (',' ReturnItem)*

ReturnItem ::= Expression ('AS' Identifier)?

OrderByClause ::= 'ORDER' 'BY' OrderItem (',' OrderItem)*

OrderItem ::= Expression ('ASC' | 'DESC')?

LimitClause ::= 'LIMIT' Integer

SkipClause ::= 'SKIP' Integer

CallClause ::= 'CALL' ProcedureName '(' (Expression (',' Expression)*)? ')'

YieldClause ::= 'YIELD' YieldItem (',' YieldItem)*

YieldItem ::= Identifier ('AS' Identifier)?

Expression ::= /* standard expression grammar */
```

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_parse_simple_match() {
    let query = "MATCH (n:Person) RETURN n";
    let ast = parse(query).unwrap();
    assert_eq!(ast.match_patterns.len(), 1);
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_execute_match_return() {
    let engine = Engine::new().unwrap();
    // Insert test data
    let result = engine.execute("MATCH (n:Person) RETURN n LIMIT 10").await.unwrap();
    assert_eq!(result.rows.len(), 10);
}
```

### Compliance Tests

TPC-like graph query suite (future V1):
- Social network queries (friends, recommendations)
- E-commerce queries (products, orders)
- Knowledge graph queries (entities, relationships)

## Future Extensions

### V2: Advanced Cypher

- WITH clause (query piping)
- OPTIONAL MATCH (left join semantics)
- UNION / UNION ALL
- Subqueries in WHERE (EXISTS, ANY, ALL)
- Map and list operators

## Advanced Types (v1.5 — phase6_opencypher-advanced-types)

### BYTES scalar

Nexus represents binary data as the JSON shape
`{"_bytes": "<base64>"}`. The following scalar functions are
available in every expression position:

| Function                              | Result                                    |
|---------------------------------------|-------------------------------------------|
| `bytes(str)`                          | UTF-8 encode a STRING to BYTES            |
| `bytesFromBase64(str)`                | Decode a base64 STRING to BYTES (bounded) |
| `bytesToBase64(b)`                    | Encode BYTES as a base64 STRING           |
| `bytesToHex(b)`                       | Lowercase hex of the bytes                |
| `bytesLength(b)`                      | Length in bytes (INTEGER)                 |
| `bytesSlice(b, start, len)`           | Sub-range, clamped like `substring`       |

NULL in → NULL out across every entry point. The per-property cap
is 64 MiB; exceeding it raises `ERR_BYTES_TOO_LARGE`.

**Base64 payload bounded allocation:** Base64-encoded BYTES literals and `$parameter` values are validated on their **encoded length** before decoding (before per-property size checks apply), rejecting oversized inputs with a Cypher error. This prevents a query from allocating multi-gigabyte buffers via a large base64 string in a literal or parameter binding.

### Dynamic labels and relationship types

`$param` is accepted wherever a label or relationship type appears in read and write clauses.

#### Labels and types — read side (MATCH patterns)

```cypher
-- Match nodes with a dynamic label
MATCH (n:$label)
WHERE n.id = 42
RETURN n

-- Variable-length paths with dynamic types
MATCH (a)-[:$reltype*1..5]->(b)
RETURN a, b

-- Multiple dynamic relationship types (union)
MATCH (a)-[:R1|$type2|R3]->(b)
RETURN a, b
```

Dynamic labels and types in reads resolve at execution time against query parameters:
- **STRING parameter:** single label/type
- **LIST<STRING> parameter:** multiple labels (node must carry ALL as a label intersection), or union of relationship types (matched if edge is any of the types)
- **NULL, missing, empty, or non-STRING:** returns zero rows (no error)
- A LIST containing non-STRING elements raises `ERR_INVALID_LABEL` / `ERR_INVALID_RELATIONSHIP_TYPE`

#### Labels and types — write side

`$param` is accepted wherever a label or relationship type appears in a write clause:

```cypher
CREATE (n:$label)
CREATE (n:Base:$role)
SET n:$label
REMOVE n:$label

CREATE (a)-[r:$type]->(b)
MERGE (a)-[r:$reltype]->(b) ON CREATE SET r.created = true
```

The parameter may be a STRING (single label/type) or a LIST<STRING>
(expands to multiple labels/types in order). Rejected with
`ERR_INVALID_LABEL` / `ERR_INVALID_RELATIONSHIP_TYPE`: NULL, empty string, empty list, non-STRING
list element, or a label/type string containing characters outside
`[A-Za-z_][A-Za-z0-9_]*`.

### Composite B-tree indexes

```cypher
CREATE INDEX person_tenant_id FOR (p:Person) ON (p.tenantId, p.id)
```

Seek modes: exact (every column bound), prefix (leading columns
bound), range on the first unbound column. `UNIQUE` flag
supported; a duplicate tuple raises `ERR_CONSTRAINT_VIOLATED`.

### Transaction savepoints

See [../guides/SAVEPOINTS.md](../guides/SAVEPOINTS.md). Statements:
`SAVEPOINT name`, `ROLLBACK TO SAVEPOINT name`,
`RELEASE SAVEPOINT name`.

### Graph scoping

`GRAPH[<name>]` as a leading clause scopes the query to a named
database:

```cypher
GRAPH[analytics] MATCH (n:Person) RETURN count(n)
```

Exactly one such clause may appear, and only at the top. Missing
or inaccessible graphs raise `ERR_GRAPH_NOT_FOUND`.

### V3: Graph Algorithms

- Shortest path: `shortestPath((n)-[*]-(m))`
- All paths: `allShortestPaths((n)-[*]-(m))`
- Graph algorithms: PageRank, community detection, centrality

## References

- OpenCypher: https://opencypher.org/
- Neo4j Cypher Manual: https://neo4j.com/docs/cypher-manual/current/
- Cypher Style Guide: https://neo4j.com/developer/cypher-style-guide/

