## 1. Investigation
- [x] 1.1 Trace the cypher handler parameter plumbing from `/cypher` body into the executor
- [x] 1.2 Locate where `$param` placeholders are (or should be) resolved against the parameters map
- [x] 1.3 Confirm root cause for inline-map form `MATCH (s {id: $id})`
- [x] 1.4 Confirm root cause for WHERE form `WHERE s.id = $id`

## 2. Implementation
- [x] 2.1 Bind parameter values into inline property-map predicate evaluation
- [x] 2.2 Bind parameter values into WHERE comparison evaluation
- [x] 2.3 Return a structured error when a referenced parameter is missing

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the implementation
- [x] 3.2 Write tests: parametrized inline-map and WHERE forms return same rows as inlined literal
- [x] 3.3 Run tests and confirm they pass
