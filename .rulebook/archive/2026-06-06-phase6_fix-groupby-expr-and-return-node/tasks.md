## 1. Investigation
- [x] 1.1 Locate implicit-GROUP-BY key derivation in the projection/aggregation operator
- [x] 1.2 Confirm why expression keys (labels(n)[0]) are not treated as grouping keys (parser dropped postfix [index] after a function call)
- [x] 1.3 Locate node-variable serialization in RETURN/projection and confirm why RETURN t yields null (server CREATE/MERGE-RETURN hand-rolled path returned Null for non-PropertyAccess/Literal)

## 2. Implementation — GROUP BY by expression
- [x] 2.1 Treat every non-aggregating projection term (including expressions) as an implicit grouping key (parser now folds postfix [index]/[start..end] after function calls into ArrayIndex/ArraySlice)
- [x] 2.2 Apply the rule in both RETURN and WITH projections
- [x] 2.3 Emit output column names from projection aliases (label, c)

## 3. Implementation — RETURN nodeVar
- [x] 3.1 Serialize a bound node variable to the standard node object ({…properties, _nexus_id}, matching read_node_as_value)
- [x] 3.2 Verify CREATE ... RETURN t and MATCH ... RETURN t both return the node object

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 4.1 Update or create documentation covering the implementation (CHANGELOG Unreleased / GH #5)
- [x] 4.2 Write tests: expression-keyed aggregation groups correctly (tests/cypher_groupby_expression_key_test.rs); RETURN nodeVar returns full node (nexus-server cypher tests)
- [x] 4.3 Run tests and confirm they pass (3 groupby tests + server RETURN-node test; nexus-core lib serial 2354 green; clippy clean)
