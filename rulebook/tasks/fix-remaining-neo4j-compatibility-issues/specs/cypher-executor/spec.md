# Cypher Executor Specification - Compatibility Fixes

## Purpose

This specification defines the remaining compatibility fixes needed to achieve 100% Neo4j compatibility in Cypher query execution. These fixes address edge cases in operator ordering, UNION clause handling, multi-label matching, and nested aggregations.

## Requirements

### Requirement: UNWIND with Aggregation Support

The system SHALL correctly execute queries that combine UNWIND with aggregation functions, ensuring proper operator ordering and result handling.

#### Scenario: UNWIND with DISTINCT and ORDER BY

Given a query with UNWIND followed by DISTINCT and ORDER BY
When the query is executed
Then the system SHALL:
- Execute UNWIND first to expand collections into rows
- Apply DISTINCT to remove duplicate values
- Apply ORDER BY to sort the final results
- Return the correct number of distinct, sorted rows

**Example Query**:
```cypher
MATCH (n) UNWIND labels(n) AS label
RETURN DISTINCT label
ORDER BY label
```

### Requirement: LIMIT after UNION

The system SHALL correctly apply LIMIT clauses to the combined result set from UNION operations, not to individual branches.

#### Scenario: LIMIT on Combined UNION Results

Given a UNION query with a LIMIT clause
When the query is executed
Then the system SHALL:
- Combine results from all UNION branches first
- Apply LIMIT to the combined result set
- Return at most LIMIT rows from the combined results
- Preserve correct ordering if ORDER BY is also present

**Example Query**:
```cypher
MATCH (a:A) RETURN a.n AS n
UNION
MATCH (b:B) RETURN b.n AS n
LIMIT 5
```

### Requirement: ORDER BY after UNION

The system SHALL correctly apply ORDER BY clauses to the combined result set from UNION operations.

#### Scenario: ORDER BY on Combined UNION Results

Given a UNION query with an ORDER BY clause
When the query is executed
Then the system SHALL:
- Combine results from all UNION branches first
- Apply ORDER BY to the combined result set
- Sort all results according to the specified columns and direction
- Support multiple sort columns and DESC ordering

**Example Query**:
```cypher
MATCH (a:A) RETURN a.name AS name
UNION
MATCH (b:B) RETURN b.name AS name
ORDER BY name
```

### Requirement: Multi-Label Node Matching

The system SHALL correctly match nodes that have ALL specified labels, preventing result duplication.

#### Scenario: Multi-Label with Relationship Traversal

Given a MATCH query with multiple labels and relationships
When the query is executed
Then the system SHALL:
- Match only nodes that have ALL specified labels (AND logic)
- Traverse relationships correctly from multi-label nodes
- Apply WHERE clause filters correctly
- Return exactly one row per matching pattern (no duplicates)

**Example Query**:
```cypher
MATCH (p:Person:Employee)-[r:WORKS_AT]->(c:Company)
WHERE r.role = 'Developer'
RETURN p.name AS employee, c.name AS company, r.since AS started
```

### Requirement: Nested Aggregation Functions

The system SHALL correctly evaluate non-aggregate functions applied to aggregation results in post-aggregation projection.

#### Scenario: List Functions on collect() Results

Given a query with head/tail/reverse applied to collect() aggregation
When the query is executed
Then the system SHALL:
- Execute the aggregation first (e.g., collect())
- Store the aggregation result in a temporary variable
- Apply the list function (head/tail/reverse) to the aggregation result
- Return the correct transformed value

**Example Queries**:
```cypher
MATCH (n:Person) RETURN head(collect(n.name)) AS first_name
MATCH (n:Person) RETURN tail(collect(n.name)) AS remaining
MATCH (n:Person) RETURN reverse(collect(n.name)) AS reversed
```

## Implementation Notes

### Operator Ordering

The planner MUST ensure correct operator ordering:
- For UNWIND + aggregation: `UNWIND` → `Aggregate` → `Distinct` → `Sort` → `Limit`
- For UNION + LIMIT: `Union` → `Limit`
- For UNION + ORDER BY: `Union` → `Sort` → `Limit` (if LIMIT also present)

### Multi-Label Matching

Multi-label matching MUST use AND logic:
- `MATCH (n:Person:Employee)` matches nodes with BOTH Person AND Employee labels
- NOT nodes with Person OR Employee labels

### Post-Aggregation Projection

Post-aggregation projection MUST:
- Access aggregation results via their aliases
- Evaluate expressions in the context of aggregated rows
- Support nested function calls on aggregation results

