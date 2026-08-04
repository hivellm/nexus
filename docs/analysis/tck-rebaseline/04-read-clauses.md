# 04 — Read clauses

**~500 attributed failures** across MATCH, WITH, ORDER BY, SKIP/LIMIT. Two are parser
gaps with outsized reach; two are validation; one is the plan's only remaining `L`.

---

## F-130 — RC07: `p = pattern` parses only as the first element of a pattern list · **66 fails** · S

```
MATCH r = ()-[]-() RETURN r            → parses          ✓
MATCH (), r = ()-[]-() RETURN r        → Cypher syntax error: Expected '(' at column 12   ✗
MATCH ()-[]-(), r = ()-[]-(), () …     → same                                             ✗
```

The path-assignment prefix is recognised by `parser/clauses/read.rs:14-37` only at the
head of the clause; after a comma the pattern-list parser expects `(` immediately.

**Why it matters far beyond parsing:** 64 of these 66 are `clauses/match` **negative**
tests whose assertion is `expected error to contain VariableTypeConflict`. They fail
today with a *parse* error instead. Fixing the parser converts them into RC06's bucket —
they only pass once **both** land. RC07 must therefore be sequenced **before** RC06,
and RC06's measured recovery will appear only after RC07.

**Owning task:** `phase22_tck-parse-path-assignment-in-pattern-list`.
**Confidence.** High.

---

## F-131 — RC06: no variable-reuse / type-conflict validation · **96 fails** · M

Reusing one variable for two different entity kinds, in the same pattern or across
clauses, must fail at compile time. Nexus accepts all of it and returns 0 rows:

```
MATCH r = ()-[]-()  MATCH (r) RETURN r        → succeeds, 0 rows   (want SyntaxError)
MATCH (p)-[]-()     MATCH p = ()-[]-() RETURN p → succeeds, 0 rows (want SyntaxError)
MATCH r = ()-[]-(), (r) RETURN r              → succeeds, 0 rows   (want SyntaxError)
MATCH (a) CREATE (a)                          → succeeds           (want SyntaxError)
```

Scenario families and weights: *node has the same variable in the same pattern* 20,
*relationship … same pattern* 20, *relationship … preceding MATCH* 15, *node …
preceding MATCH* 14, *path … preceding MATCH* / *same pattern* (Match1) 45, *matching a
path variable bound to a value* 8.

Required detail tokens: `VariableTypeConflict` when the kinds differ (node vs
relationship vs path), `VariableAlreadyBound` when a bound variable is re-bound by a
`CREATE`/`MERGE`, `InvalidParameterUse` for `MATCH (n $param)`. The semantic-validation
pass exists (`executor/semantic_validation/mod.rs`, and it already reads
`pattern.path_variable` at three sites) — this is a **new check inside an existing
stage**, not new infrastructure.

**Blocked by:** RC07 (F-130).
**Owning task:** `phase22_tck-validate-variable-reuse`.
**Confidence.** High.

---

## F-132 — RC08: a *positive* pattern predicate in `WHERE` does not parse · **34 fails** · S

```
MATCH (n) WHERE (n)-[:T]->()       RETURN n → syntax error at the '-' ✗
MATCH (n) WHERE NOT (n)-[:T]->()   RETURN n → works                   ✓
MATCH (n) WHERE exists { (n)-[:T]->() } RETURN n → works              ✓
MATCH (n) WHERE exists((n)-[:T]->()) RETURN n → syntax error          ✗
```

A pattern is accepted as a predicate **only** after `NOT` or inside `exists { … }`.
Bare and function-style `exists(pattern)` both fail. The machinery to evaluate a
pattern predicate is therefore already wired (the `NOT` path proves it) — this is a
grammar entry point, not a feature.

Nearly all of `expressions/pattern` (31 of 50 scenarios) plus singles in
`match-where`, `with-where`, and `existentialSubqueries`.

**Owning task:** `phase22_tck-parse-positive-pattern-predicate`.
**Confidence.** High.

---

## F-133 — RC13: SKIP / LIMIT semantics · **89 fails** · S

Four independent defects, one owner:

```
MATCH (n) RETURN n LIMIT 0                    → 3 rows      (want 0 — LIMIT 0 read as "no limit")
MATCH (n) RETURN n ORDER BY n.name SKIP $skipAmount → 5 rows (want 3 — parameter ignored)
MATCH (p:Person) RETURN p.name SKIP 1.5       → succeeds    (want SyntaxError)
UNWIND [1,2,3,4] AS i WITH i LIMIT 2 RETURN sum(i) → 10      (want 3 — LIMIT dropped before an aggregation)
MATCH (a) WITH a, a.bool AS b WITH a, b ORDER BY b LIMIT 3 RETURN a, b → 5 rows (want 3)
```

`LIMIT 0` and the parameter form are one-liners. The "dropped before an aggregation"
case is the 58 with-orderBy rows and interacts with RC02 (a collapsed grouping key also
collapses the row count) — **verify against RC02 before sizing.** Negative rows want
`NegativeIntegerArgument` for `SKIP -1` and `InvalidArgumentType` for a float.

**Owning task:** `phase22_tck-skip-limit-semantics`.
**Confidence.** High on the reproductions; medium on the 89 (shared with RC02/RC17).

---

## F-134 — RC12: no validation for ORDER BY on an aggregate or an out-of-scope variable · **46 fails** · S

```
… WITH a ORDER BY count(a)     → succeeds   (want SyntaxError)
… WITH a AS b ORDER BY a       → succeeds   (want SyntaxError — `a` is out of scope after the WITH)
```

Weights: *Fail on sorting by an aggregation* 25, *… by any number of undefined
variables in any position* 13, *… by an undefined variable / out of scope* 5+.

`WITH` narrows scope: only its projected aliases are visible to its own `ORDER BY`,
`SKIP`, `LIMIT`, and everything downstream. This is the same scope model RC06 needs, so
the two tasks should share the scope-tracking helper — but they are independently
shippable.

**Owning task:** `phase22_tck-validate-orderby-scope`.
**Confidence.** High.

---

## F-137 — RC16: ordering across types, lists, and instants · **49 fails** · M

```
UNWIND [[], ['a'], ['a',1], [1], [1,'a'], [1,null], [null,1], [null,2]] AS l
WITH l ORDER BY l LIMIT 4 RETURN l
  → row[1] = [1]                       (want ['a'])

UNWIND [1,'a',null,[1,2],0.2,'b'] AS x RETURN min(x), max(x)
  → 'b', [1,2]                         (want [1,2], 1)
```

Three sub-problems: (a) the total order across *value types* (the openCypher order
places numbers before strings before booleans before lists, with `null` last
ascending), (b) element-wise ordering **inside** lists including nulls, (c) instant
ordering for offset-aware temporals (F-123). `min()`/`max()` must use the same
comparator as `ORDER BY`; today they disagree with it and with each other.

An earlier task (`phase21_tck-order-by-expression-re-evaluation`) fixed cross-type
ordering for the *scalar* case; this extends the same comparator to lists, temporals,
and the aggregate functions.

**Owning task:** `phase22_tck-ordering-cross-type-and-instant`.
**Confidence.** High.

---

## F-138 — RC09: a named path variable is unbound for fixed-length patterns · **14 fails** · M

```
MATCH p = (a) RETURN p                          → null
MATCH p = (a:Q)-->(b) RETURN p                  → null
MATCH p = (a:Q)-->(b) RETURN nodes(p), relationships(p), length(p) → null, null, null
```

**Confirmed in source:** `pattern.path_variable` is consumed at exactly one place in the
planner — `planner/queries/relationships.rs:220`, which builds
`Operator::VariableLengthPath`. The fixed-length branch a few lines below emits
`Operator::Expand` and **drops the path variable entirely**, and the zero-length case
(`MATCH p = (a)`) never reaches a relationship operator at all. So a path value exists
only for `*`-quantified patterns.

Gates all 20 positive scenarios of `Match6.feature`, `MERGE p = (…)`, and
`OPTIONAL MATCH p = (…)`. Also the reason `expressions/path` (7 scenarios, 100%) is not
evidence of path support — it never binds one from a pattern.

**Owning task:** `phase22_tck-named-path-value`.
**Confidence.** High (source-confirmed).

---

## F-139 — RC22: OPTIONAL MATCH partial bindings and var-length relationship identity · **57 fails** · L

The plan's only remaining `L`, and the one place where the failures are genuinely
semantic rather than surface-level. Three intertwined defects:

**(a) a failed OPTIONAL MATCH leaks partial bindings** — the known open issue
(`project-expand-required-partial-binding-leak`), here in its OPTIONAL form:

```
MATCH (a:A), (c:C) OPTIONAL MATCH (a)-->(b)-->(c) RETURN b
  → the C node                            (want null — the two-hop pattern has no match)
MATCH (a:Single) OPTIONAL MATCH (a)-[*]->(b) RETURN b
  → includes the Single node itself        (want only the B nodes)
```

**(b) a var-length relationship variable must always be a list, even at `*1..1`:**

```
MATCH (a)-[r*1..1]->(b) RETURN r  → a relationship  (want a one-element list)
```

**(c) var-length traversal counts and reuses relationships wrongly:**

```
MATCH (a:Blue)-[r*]->(b:Green) RETURN count(r)          → 3   (want 1)
MATCH ()-->() WITH 1 AS x MATCH ()-[r1]->()<--() RETURN sum(r1.times) → 97 (want 776)
MATCH ()-[r:EDGE]-() MATCH p = (n)-[*0..1]-()-[r]-()-[*0..1]-(m) RETURN count(p) → 0 (want 32)
```

(c) needs the relationship-isomorphism scope from `phase21_tck-…-isomorphism` extended
across var-length segments and bound relationship reuse. Expect this task to need its
own decomposition; it is the one item here that should not be attempted in a single
pass.

**Owning task:** `phase22_tck-optional-match-and-varlength-bindings`.
**Confidence.** High on the reproductions; medium on 57 as a recovery estimate.

---

## F-140 — An aggregate nested in an expression loses the row over empty input · *(inside RC22's residue)*

```
MATCH (a:ZZZ) RETURN count(a)       → 1 row, [0]    ✓
MATCH (a:ZZZ) RETURN count(a) > 0   → 0 rows        ✗ (want 1 row, [false])
```

A bare aggregate over an empty input correctly yields one row; wrap it in **any**
expression and the row disappears. Small count, trivial repro, folded into the
grouping-key task since the fix site is the same aggregating-projection code path.

**Owning task:** `phase22_tck-aggregation-grouping-key`.
**Confidence.** High.
