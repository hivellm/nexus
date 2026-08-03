## 1. Implementation
- [ ] 1.1 Accept optional whitespace between `:` and the label identifier in node patterns (`(dur2: Duration2)`); audit relationship-type position for the same gap
- [ ] 1.2 Verify no collateral: `WHERE n:Label` predicates, map literals `{key: value}`, and parameter syntax unaffected; Temporal8[6]'s 9 rows unblock

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
