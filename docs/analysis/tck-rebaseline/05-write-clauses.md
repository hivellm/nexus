# 05 — Write clauses

**191 attributed failures.** One architectural cause dominates, and it is the reason
`clauses/set` (32%), `clauses/remove` (18%), and `clauses/merge` (33%) sit far below
the read clauses despite the write path passing the Neo4j differential suite.

---

## F-150 — RC10: write queries execute on a restricted pipeline · **67 fails** · L

Write queries do not run through the general read pipeline. They take a narrower path
that re-implements a subset of it, and refuses the rest **at runtime** with messages
that read as deliberate limits:

| Query | Error |
|---|---|
| `MATCH (n:A) WHERE n.name = 'Andres' SET n.name = 'Michael' RETURN n` | `WHERE in a write query only supports id(var) = <value>` |
| `OPTIONAL MATCH (a:DoesNotExist) SET a.num = 42 RETURN a` | `OPTIONAL MATCH not supported in write queries` |
| `WITH 42 AS var MERGE (c:N {var: var})` | `Unsupported clause in write query` |
| `MATCH ()-[r]->() REMOVE r.num RETURN …` | `Unknown variable 'r' in REMOVE clause` |
| `MATCH (n) REMOVE n.num RETURN n.num IS NOT NULL AS still_there` | `Unexpected character in expression` (the *write* parser lacks `IS NOT NULL`) |
| `CREATE (:X) CREATE (:X) MERGE (:X)` | `MERGE requires a variable alias` |
| `MERGE p = (a {num: 1}) RETURN p` | `Expected '(' at column 8` |

Category split: `set` 26, `remove` 24, `merge` 13, `create` 2, plus 2 elsewhere. The
`OPTIONAL MATCH` half additionally explains RC22's `Ignore null when deleting/removing`
failures — `OPTIONAL MATCH (a:X) DELETE a RETURN a` returns 0 rows where the spec wants
one null row.

**This is the plan's only genuinely architectural item.** The right shape is *not* to
keep widening the restricted path one clause at a time — that is how it reached seven
distinct refusal messages. It is to make the write clauses **operators inside the
general pipeline**, so `WHERE`, `WITH`, `OPTIONAL MATCH`, expression parsing, and
variable binding are inherited rather than reimplemented.

That is too large for one task. Decompose along the refusal messages, upstream first:
(1) full expression/`WHERE` parsing in a write query, (2) relationship variables
visible to `SET`/`REMOVE`/`DELETE`, (3) `WITH` before a write clause, (4) `OPTIONAL
MATCH` + null-row write semantics, (5) `MERGE` without an alias and `MERGE p = …`. Each
step is independently shippable and independently measurable against the differential
suite, which must stay at 300/300 throughout.

**Owning task:** `phase22_tck-write-path-general-pipeline` (expects sub-decomposition).
**Confidence.** High on the cause; the 67 is a floor, since some scenarios are gated
twice.

---

## F-151 — RC20: missing negative-test validation, miscellaneous · **96 fails** · M

Heterogeneous but mechanical — each is "the engine accepts something the spec rejects",
and each needs a check plus a detail token. Grouped by fix site:

**Function and projection validation** (`return` 4, `graph` 2, `quantifier` 12):

```
MATCH (a) RETURN foo(a)               → succeeds  (want SyntaxError — unknown function)
MATCH (r) RETURN type(r)              → succeeds  (want SyntaxError — node passed to type())
RETURN none(x IN ['Clara'] WHERE x % 2 = 0) → succeeds (want SyntaxError — modulo on a string)
```

**Literal bounds** (`literals` 24):

```
RETURN -9223372036854775808 AS literal → syntax error (want the value — this is i64::MIN)
RETURN  9223372036854775808 AS literal → "Invalid integer literal" (want token IntegerOverflow)
RETURN -9223372036854775809 AS literal → "Invalid integer literal" (want token IntegerOverflow)
```

The first is a **real bug**: `i64::MIN` is a valid literal and is rejected because the
magnitude is parsed before the sign is applied. The other two need only the token.

**Write-side validation** (`delete` 4, `merge` 6, `create` 3):

```
MATCH (a) DELETE x                     → succeeds  (want SyntaxError — undefined variable)
MATCH (n) DELETE n:Person              → succeeds  (want SyntaxError — DELETE takes no label)
MATCH (a) CREATE (a)                   → succeeds  (want SyntaxError — already bound)
CREATE ()-->()                         → Uncategorized (want SyntaxError kind)
CREATE (a)<-[:FOO]->(b)                → wrong message (want token RequiresDirectedRelationship)
```

**List/map argument validation** (`list` 15, `boolean` 8, `map` 6): argument counts and
kinds for `range`, `reduce`, list slices, and map access.

Best executed as **one task with the four groups as checklist items**, because they
share the classification/token plumbing. Note the overlap with RC11 (F-111): both add
strictness; land RC11 first so the error-kind table exists.

**Owning task:** `phase22_tck-validate-negative-tests`.
**Confidence.** High.

---

## F-152 — RC19: parser gaps in literals, identifiers, and slices · **78 fails** · M

The residue of the grammar after RC07, RC08, and RC14 are removed:

```
WITH {name:'Mats'} AS map RETURN map.`name` AS result → Expected identifier   ✗
… list slice / index forms                             → Expected ']'   (21 rows)
… parenthesised precedence forms                       → Expected ')'   (21 rows)
```

Backtick-delimited identifiers after `.` are the clearest single item (`expressions/map`);
the `]`/`)` families are mostly list-slice and precedence forms that need the postfix
work from RC14 to be meaningful. **Sequence after RC14** and re-measure — part of this
78 will disappear when RC14 lands.

**Owning task:** `phase22_tck-parse-literals-and-identifiers`.
**Confidence.** Medium (count is RC14-contaminated).

---

## F-155 — RC21: side-effect counting · **28 fails** · S

```
MERGE (a:TheLabel {num: 43}) RETURN a.num
  got  labels_added: 1
  want labels_added: 0      ← a label applied as part of creating the node is not a separate label add
```

`merge` 12, `delete` 11, `create` 3, `remove` 1, `set` 1. An earlier task
(`phase21_tck-side-effect-counting-semantics`) established the counting model; these are
the cases it did not cover — chiefly MERGE-on-create label accounting and DELETE
counters, plus one `expected no side effects, but the query reported …`.

Small, self-contained, and a good early win because the assertions are exact.

**Owning task:** `phase22_tck-side-effect-counting-completion`.
**Confidence.** High.
