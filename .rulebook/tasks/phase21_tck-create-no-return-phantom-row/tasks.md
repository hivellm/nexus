## 1. Implementation

The executor's standalone-CREATE fast path synthesizes a result row from named
CREATE variables even when the statement has NO `RETURN` clause — precise
anchor: `executor/dispatch/operator_loop.rs`, the `!columns.is_empty()` block
(~lines 226-256) populates `context.result_set` from `created_node_ids.keys()`
unconditionally, with no check for a downstream `Project`. A write-only
statement must return zero rows. This blocks TCK Create2[2]/[3]/[5]-[12]
(10 scenarios, all with correct side effects since commit 26fb15a4) on
"Then the result should be empty — got 1 rows". Control: Create2[1] (anonymous
nodes only, no bound variables) passes. Likely related to the analysis note
that clauses/delete sits at 0% due to a phantom row on write-only statements —
verify DELETE while here (docs/analysis/tck/06-clauses-read-write.md).

- [ ] 1.1 write-only CREATE statements (no RETURN) produce an empty result set; statements WITH a RETURN/Project keep their rows exactly as today
- [ ] 1.2 check the other write-only statement families for the same phantom-row synthesis (DELETE, SET, REMOVE, MERGE without RETURN — the TCK "And no side effects"/"result should be empty" assertions) and fix whichever share the mechanism

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
