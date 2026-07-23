# Tasks: phase0_fix-where-in-prefix-param-index-seek

Remaining WHERE index-seek forms split out of
`phase0_fix-where-clause-index-seek-extensions` (range `>`/`>=`/`<`/`<=` +
EXPLAIN accuracy landed there). The B-tree already supports the primitives:
`find_exact` (for `IN`) and `find_prefix` (for `STARTS WITH`). Mirror the
`where_range_seek_operand` / `NodeIndexRangeSeek` shape the parent added. All
plan-selection only — results must not change.

## 1. `IN` seek
- [ ] 1.1 Reproduce: `MATCH (n:Person) WHERE n.age IN [1,2,3]` full-scans today.
- [ ] 1.2 Add an `Operator::NodeIndexInSeek { values }` (union of `find_exact`)
  + `where_in_seek_operand` lift + dispatch/cost arms.
- [ ] 1.3 Result-parity + plan tests.

## 2. `STARTS WITH` seek
- [ ] 2.1 Reproduce: `WHERE n.name STARTS WITH 'A'` full-scans today.
- [ ] 2.2 Add a prefix-seek operator over `find_prefix` + lift + arms.
- [ ] 2.3 Result-parity + plan tests.

## 3. `$parameter` equality seek
- [ ] 3.1 Reproduce: `WHERE n.prop = $x` full-scans (no plan-time value; the
  correlated `key_expression` path needs driving rows a bare first scan lacks).
- [ ] 3.2 Add a parameter-aware seek that resolves the value from
  `context.params` at execution time without requiring driving rows.
- [ ] 3.3 Result-parity + plan tests.

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation
- [ ] 4.2 Write tests covering the new behavior
- [ ] 4.3 Run tests and confirm they pass

## Related
- `phase0_fix-where-clause-index-seek-extensions` — parent (range + EXPLAIN)
