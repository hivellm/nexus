## 1. Investigation
- [ ] 1.1 Confirm the single-transaction materialization at mod.rs:4998 (`_batch_size` unused, `all_results.extend` before one commit) and pick a sensible result-count cap / config knob
- [ ] 1.2 Define the structured error (`ERR_CALL_IN_TX_RESULT_TOO_LARGE`) and where to surface it

## 2. Implementation
- [ ] 2.1 Cap the materialized subquery result count and return the structured error past the cap (no silent OOM); document `OF n ROWS` granularity as not-yet-implemented
- [ ] 2.2 Ensure the error is a clean rollback (no partial commit) and is returned over REST as an execution error

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation (CHANGELOG / GH #22)
- [ ] 3.2 Write tests: a CALL IN TRANSACTIONS subquery exceeding the cap returns the structured error and commits nothing; under-cap still works
- [ ] 3.3 Run tests and confirm they pass
