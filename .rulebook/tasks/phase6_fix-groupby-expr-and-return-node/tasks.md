## 1. Investigation
- [ ] 1.1 Locate implicit-GROUP-BY key derivation in the projection/aggregation operator
- [ ] 1.2 Confirm why expression keys (labels(n)[0]) are not treated as grouping keys
- [ ] 1.3 Locate node-variable serialization in RETURN/projection and confirm why RETURN t yields null

## 2. Implementation — GROUP BY by expression
- [ ] 2.1 Treat every non-aggregating projection term (including expressions) as an implicit grouping key
- [ ] 2.2 Apply the rule in both RETURN and WITH projections
- [ ] 2.3 Emit output column names from projection aliases (label, c)

## 3. Implementation — RETURN nodeVar
- [ ] 3.1 Serialize a bound node variable to the standard node object (id + labels + properties)
- [ ] 3.2 Verify CREATE ... RETURN t and MATCH ... RETURN t both return the node object

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation
- [ ] 4.2 Write tests: expression-keyed aggregation groups correctly; RETURN nodeVar returns full node
- [ ] 4.3 Run tests and confirm they pass
