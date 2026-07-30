## 1. Implementation
- [ ] 1.1 Materialize comma-pattern variables into driving rows (cartesian product across patterns) so downstream operators see them row-bound
- [ ] 1.2 Verify OPTIONAL MATCH closing over a comma-pattern variable pads instead of rebinding (WithWhere1[3]/[4] pass; no regression in existing multi-pattern MATCH suites)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
