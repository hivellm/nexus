# 02 — Projection & expressions

The largest cluster and the cheapest. Seven root causes, **~880 attributed failures**,
none of which needs new infrastructure.

---

## F-101 — RC01: a list quantifier in a standalone projection returns zero rows · **479 fails** · S

**The single biggest defect in the corpus.** Reproduced live:

```
RETURN all(x IN [1,2] WHERE x > 0) AS a          →  0 rows          ✗
RETURN all(x IN [1,2] WHERE true)  AS a          →  1 row  [true]   ✓
RETURN none(x IN [] WHERE true)    AS a          →  1 row  [true]   ✓
UNWIND [1] AS z RETURN all(x IN [1,2] WHERE x > 0) AS a → 1 row [true] ✓
WITH 1 AS z     RETURN all(x IN [1,2] WHERE x > 0) AS a → 1 row [true] ✓
RETURN [x IN [1,2] WHERE x > 0] AS a             →  1 row  [[1,2]]   ✓
```

The pattern is exact: **the row is lost only when (a) the projection is standalone —
no `MATCH`/`WITH`/`UNWIND` upstream — and (b) the quantifier's predicate references the
quantifier's own bound variable.** A constant predicate survives. A list comprehension
binding the same variable survives. Any upstream clause makes it work.

This is why `expressions/quantifier` reads 8.1%: `Quantifier1..4.feature` are
`RETURN <quantifier>` outlines, 101 failures each.

The evaluator is **not** the problem — `eval/projection/fn_list.rs:311-470` implements
`all`/`any`/`none`/`single` correctly, and `Project` propagates evaluation errors with
`?` rather than dropping rows (`operators/project.rs:393,505`), so the row cannot be
lost inside the projection. The loss is upstream of it.

**Lead (unconfirmed, two candidate sites).** The quantifier's inner `WHERE` is being
lifted to a query-level predicate on the no-pattern planning path, where `x` is
unbound → `null` → the synthetic single row is filtered out. Consistent with every
observation above, including why `WHERE true` survives. Candidates:
`executor/planner/preparse.rs` (text-level clause pre-scan) and
`executor/planner/queries/planner_core/bound.rs` (the no-pattern path the archived
ORDER BY task already found handles `UNWIND … RETURN` differently). **Confirm before
fixing** — if the mechanism is instead a scan planned for the free variable `x`, the
fix site changes.

**Owning task:** `phase22_tck-quantifier-standalone-projection`.
**Confidence.** Reproduction: high. Mechanism: lead.

---

## F-103 — RC02: the aggregation grouping key is dropped unless it is a property access · **64 fails** · S

```
MATCH (n:P) RETURN n.x AS k, count(*) AS c        →  [[2,1],[1,2]]   ✓
UNWIND [1,1,2] AS v WITH v AS k, count(*) AS c RETURN k, c
                                                 →  columns ["c"], [[3]]   ✗
UNWIND [1,1,2] AS v WITH 9 AS k, count(*) AS c RETURN k, c
                                                 →  columns ["c"], [[3]]   ✗
```

When the non-aggregate item in an aggregating projection is a **property access on a
graph variable**, grouping works. When it is a plain variable, a literal, or any other
expression, the column is **discarded** and every input row collapses into one group.
The downstream `RETURN k, c` then cannot find `k` and emits `c` — which is exactly the
log's top column-name signature (`result="cnt", table="result"`, 64 rows, all in
`Quantifier10..12`).

Note this is *not* only a naming bug: the aggregate value is wrong too (3 instead of
2 and 1). The 64 count is an undercount of the semantic damage — the same defect
inflates RC13's with-orderBy rows.

**Owning task:** `phase22_tck-aggregation-grouping-key`.
**Confidence.** High (reproduced, both directions).

---

## F-110 — RC14: the projection list is silently truncated by postfix expression forms · **~90 fails** · M

Two reproductions, both from the log:

```
RETURN false = true IS NULL AS a, false = (true IS NULL) AS b, (false = true) IS NULL AS c
  →  1 column, named "false = true"                                 ✗ (want 3: a,b,c)

MERGE (a)-[r:KNOWS]-(b) RETURN startNode(r).id AS s, endNode(r).id AS e
  →  1 column, named "startNode(r)"                                 ✗ (want 2: s,e)

WITH [123, {existing: 42}] AS list RETURN (list[1]).missing, (list[1]).existing
  →  1 column, named "list[1]"                                      ✗ (want 2)
```

The projection-list parser, on reaching an expression form it cannot continue
(`… IS NULL` mid-comparison, `.prop` applied to a call or a parenthesised expression),
**stops, keeps what it has, and discards the rest of the list — including the `AS`
alias.** No error is raised.

A worse variant loses a whole clause:

```
UNWIND [true] AS a WITH collect(a) AS eq RETURN all(x IN eq WHERE x) AND any(x IN eq WHERE x) AS result
  →  columns ["eq"]                                                 ✗ (the RETURN was ignored)
```

**This is the most dangerous defect in the corpus** — worse than a wrong answer,
because the engine answers a *different question* and reports success. It also makes
RC18 (column-name fidelity) unmeasurable: some "wrong column name" failures are
actually "the parser gave up here".

Fix in two parts, in this order: (1) the parser must **error** when a projection item
cannot be fully consumed, never truncate; (2) then add the missing postfix forms —
`IS NULL`/`IS NOT NULL` as a postfix operator inside a larger expression,
`.prop` after `)`/`]`, and comparison chaining.

**Owning task:** `phase22_tck-projection-list-silent-truncation`.
**Confidence.** High (three reproductions from three categories).

---

## F-111 — RC11: functions return `null` where the spec requires a typed error · **103 fails** · M

Two distinct defects sharing a fix site:

**(a) missing guards** — the function silently produces `null`/succeeds:

```
WITH [1,2,3] AS list, true AS idx RETURN list[idx]     → succeeds   (want TypeError)
RETURN [x IN [true, []]  | toBoolean(x)] AS list       → succeeds   (want TypeError)
RETURN [x IN [r, 0]      | type(x)]     AS list        → succeeds   (want TypeError)
MATCH p = (a) RETURN labels(p) AS l                    → succeeds   (want TypeError)
```

**(b) wrong `OpenCypherErrorKind`** — the error is raised but classified
`Uncategorized`:

```
WITH true AS list, 0 AS idx RETURN list[idx]
  → "ERR_INVALID_KEY: property key must be STRING (got INTEGER)" / Uncategorized
    (want TypeError)
CREATE ()-->()   → "Relationship must have a type" / Uncategorized (want SyntaxError)
```

Part (b) is mechanical: extend the classification table so these errors carry the
right kind. Part (a) is a per-function argument-type contract for the scalar, list,
and graph function families (`toBoolean`/`toInteger`/`toFloat`, `type`, `labels`,
`properties`, `keys`, list indexing, `range`, `size`).

Also here: `toInteger('1.7')` must be `1` and `toInteger('2.9')` must be `2` (parse as
float, then truncate); today both are `null`.

**Regression risk: highest in the plan.** These functions currently absorb bad input
silently, and internal tests may depend on that. Gate on the differential suite.

**Owning task:** `phase22_tck-runtime-type-guards`.
**Confidence.** High.

---

## F-112 — RC15: comparison is not three-valued inside lists · **24 fails** · S

```
RETURN [null] = [1] AS result            → false     (want null)
RETURN [[1],[2]] = [[1],[null]] AS result → false    (want null)
```

Top-level 3VL landed (`expressions/null` 95.5%, `expressions/boolean` 86.7%), but
list and nested-list equality still collapse an incomparable element to `false`
instead of propagating `null`. Same shape for `IN` over a list containing nulls.

**Owning task:** `phase22_tck-three-valued-comparison-lists`.
**Confidence.** High.

---

## F-113 — RC17: a chained `WITH` alias rename yields null · **correctness hazard, ~0 TCK rows** · S

Found while probing RC13, not present as its own TCK signature:

```
UNWIND [5,1,4] AS i WITH i AS a RETURN a           → [5,1,4]              ✓
UNWIND [5,1,4] AS i WITH i AS a WITH a RETURN a    → [null,null,null]     ✗
UNWIND [5,1,4] AS i WITH i AS a WITH a AS b RETURN b → [null,null,null]   ✗
UNWIND [5,1,4] AS i WITH i WITH i RETURN i         → [5,1,4]              ✓
MATCH (n:P) WITH n.x AS a WITH a RETURN a          → [1,1,2]              ✓
```

An `UNWIND`-bound variable **renamed** by a `WITH` and then re-projected by a *second*
`WITH` loses its value. Renaming once is fine; renaming then re-projecting is not.
Re-projecting without renaming is fine.

Its TCK footprint is small only because few scenarios chain two `WITH`s over an
`UNWIND`. As a product defect it is severe — silent data loss in a common shape — so
it is scheduled first despite the low scenario count.

**Owning task:** `phase22_tck-chained-with-alias-null`.
**Confidence.** High (reproduced, five narrowing variants).

---

## F-114 — RC18: column-name fidelity residue · **47 fails** · S

After RC14 is fixed, what remains is verbatim-rendering divergence:

```
MATCH (n) RETURN (n:Foo)   → column "n:Foo"    (want "(n:Foo)")
RETURN (a AND b) IS NULL = (b AND a) IS NULL AS result
                           → column "a AND b IS NULL"  (want "result")
```

The second is RC14 leaking (the alias was lost). **Do not size or start this task
before RC14 lands** — the measured 47 will change.

**Owning task:** `phase22_tck-column-name-fidelity-residue`.
**Confidence.** Medium (count is RC14-contaminated by construction).
