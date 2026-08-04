# Proposal: phase21_tck-cross-type-comparison-semantics

> Part of the openCypher TCK 100%-conformance epic. Plan: docs/analysis/tck/.
> Surfaced while sizing `phase21_tck-non-finite-floats` (Phase 7): the NaN
> scenarios share a code path with a much cheaper and more damaging defect.

## Why

Comparing values of different types returns a confident wrong answer instead of
`null`. Measured, not inferred:

| Query | Nexus today | openCypher |
|---|---|---|
| `RETURN 1 < 'text'` | `true` | `null` |
| `RETURN '1.0' < 1.0` | `false` | `null` |
| `RETURN true < 1` | `false` | `null` |
| `RETURN [1] < 1` | `false` | `null` |
| `RETURN 1.0 = '1.0'` | `true` | **`false`** |

The cause is a single shared comparator. `<`, `<=`, `>`, `>=` are all implemented
as `compare_values_for_sort(l, r) == Ordering::Less` (etc.), and that function's
final arm stringifies both operands and compares the text — so `1 < 'text'`
becomes `"1" < "text"`, which is `true`. Nothing in the operator path asks whether
the two values are comparable at all.

This is worse than a conformance gap: it silently corrupts ordinary `WHERE`
filters over heterogeneous properties, where a spec-conformant engine drops the
row (`null` is not `true`) and Nexus keeps or discards it by string spelling. The
equality case is worse still — `1.0 = '1.0'` returning `true` means a number
matches a string that merely looks like it.

Note the two rules differ, and the TCK is explicit about it:

- **Ordering** across types is `null` — openCypher TCK
  `expressions/comparison/Comparison2.feature` [3] "Comparing across types yields
  null, except numbers", whose query filters `WHERE result` over every pair drawn
  from `[node, rel, path, '', 1, 3.14, true, null, [], {}]` and expects ONLY the
  numeric pairs to survive.
- **Equality** across types is `false`, not `null` — `Comparison1.feature` [9]
  "Equality between strings and numbers" (`'1.0' = 1.0` → `false`), while
  `1 = 1.0` → `true` because INT and FLOAT are one type family for this purpose.

## What Changes

- A type-compatibility gate in the comparison operators: `<`, `<=`, `>`, `>=`
  yield `null` unless both operands are the same kind (with INT/FLOAT counted as
  one numeric kind). `null` on either side already yields `null` and stays so.
- `=` / `<>` across different kinds yield `false` / `true` rather than a coerced
  verdict. Numeric cross-subtype equality (`1 = 1.0`) keeps working.
- The gate lives in the OPERATOR path only. `compare_values_for_sort` also backs
  `ORDER BY`, which needs a TOTAL order over every type
  (`phase21_tck-order-by-expression-re-evaluation` added that as a sort-path rank
  for exactly this reason) — the two must not be merged.

## Out of scope, sized for a follow-up

Same category, different rule (structural comparison WITHIN a type), so not
bundled here:

- `Comparison1` [7] "Comparing maps to maps" (5 scenarios) — map equality has its
  own 3VL: a differing KEY SET is `false` even when the extra value is null
  (`{} = {k: null}` → `false`), while an equal key set with a null on either side
  is `null` (`{k: 1} = {k: null}` → `null`).
- `Comparison2` [4] "Comparing lists" and `Comparison1` [6] "Comparing lists to
  lists" (4 scenarios).
- `Comparison1` [5] "Comparing relationships to relationships" (1 scenario).

## TCK Impact

`expressions/comparison` is 44/72. Directly targeted: [3] (4), [6]
"Comparability between numbers and strings" (2), [9] "Equality between strings and
numbers" (2). `Comparison2` [5] "Comparing NaN" (4) needs its `NaN vs 'a'` → null
row from this change and the rest from `phase21_tck-non-finite-floats`.

## Impact

- Affected specs: docs/specs/cypher-subset.md (comparison and equality semantics)
- Affected code: crates/nexus-core/src/executor/eval/predicate.rs (the operator
  arms), crates/nexus-core/src/executor/eval/projection/core.rs (the same
  operators on the projection path)
- Breaking change: YES, observably — a query comparing across types changes from
  a boolean to `null`, so a `WHERE` that kept rows by accident will stop. That is
  the correction, but it must be called out in the changelog.
- User benefit: heterogeneous comparisons stop producing confident wrong answers;
  `WHERE` filters agree with Neo4j.
