## 1. Implementation
- [ ] 1.1 Standalone CREATE evaluates function-call property values (route through the row-aware evaluator + canonicalize_value_in_place before storage; keep rejection only for genuinely unsupported expression classes)
- [ ] 1.2 Audit MERGE and FOREACH property paths for the same limitation; verify Temporal5.feature setup fixtures now execute (7 scenarios unblocked)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
