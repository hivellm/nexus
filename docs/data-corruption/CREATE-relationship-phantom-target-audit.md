# Data Corruption Audit: CREATE Relationship Phantom Target Nodes

**Fixed in**: 3.0.0  
**Severity**: High (silent data corruption on affected query patterns)  
**Affected Operations**: `CREATE` with relationship patterns combined with `SET`, `REMOVE`, `MERGE`, or `FOREACH` in the same query  
**Impact**: Queries may inadvertently create orphaned duplicate nodes and modify the wrong node

## The Bug

A `CREATE` statement that both:
1. Creates a relationship with an inline target node (e.g., `CREATE (a)-[:LINKS]->(b:Beta {prop: value})`)
2. Contains a write clause in the same query (`SET`, `REMOVE`, `MERGE`, or `FOREACH`)

...would create a **phantom duplicate** of the target node and bind query variables to the **orphaned copy** instead of the connected one.

### Example

```cypher
-- BUGGY (before fix): Creates TWO :Beta nodes, one connected, one orphan
CREATE (a:Alpha {id: 1})-[:LINKS]->(b:Beta {label: 'connected'}) 
SET a.prop = 'modified'
RETURN a, b
```

**What actually happened**:
1. The relationship-processing code created `b` (id=100) and wired it to the relationship
2. The node-processing code re-created `b` again (id=101) as an orphaned duplicate with identical properties
3. The variable `b` was rebound to the orphan (id=101)
4. Any reference to `b` in `SET`, `RETURN`, or downstream clauses operated on the orphan (id=101), not the connected node (id=100)

**Result**:
- Two nodes with the same label and properties exist in the database
- One is properly connected via the relationship
- One is completely unconnected (no incoming or outgoing edges)
- Query results and downstream writes reference the unconnected copy

Plain `CREATE` without subsequent write clauses was **never affected** — it uses a different code path.

## Detection Query

Use the following conservative Cypher query to find candidate orphan nodes:

```cypher
MATCH (orphan) WHERE NOT (orphan)--()
RETURN orphan
```

This returns all nodes with **no relationships** (neither incoming nor outgoing). On a healthy database, such nodes are legitimate (e.g., isolated test data, single-node clusters, nodes awaiting connection). However, if you detect many isolated nodes of a particular label or with specific properties, they may be phantom duplicates.

### Manual Audit Process

For a specific label suspected of containing phantoms:

```cypher
-- 1. Find all isolated nodes with that label
MATCH (orphan:SuspectedLabel) WHERE NOT (orphan)--()
RETURN orphan LIMIT 100
```

```cypher
-- 2. Check for connected twins with identical properties
MATCH (orphan:SuspectedLabel) WHERE NOT (orphan)--()
MATCH (connected:SuspectedLabel) WHERE (connected)--() AND connected <> orphan
RETURN orphan, connected
-- Review each pair manually to confirm if they have identical properties
```

### Property-by-Property Comparison

Since Nexus's Cypher subset does not provide a built-in `properties()` function that returns a node's full property map, you must audit specific properties you expect duplicates to share:

```cypher
-- Example: find isolated nodes matching connected twins by two key properties
MATCH (orphan:Document) WHERE NOT (orphan)--()
MATCH (connected:Document) 
WHERE (connected)--() 
  AND connected <> orphan 
  AND connected.id = orphan.id 
  AND connected.name = orphan.name
RETURN orphan, connected
ORDER BY orphan.id
```

Adjust `connected.id = orphan.id AND connected.name = orphan.name` to include the properties your queries typically set on new nodes.

## Cleanup Guidance

**CAUTION**: Before deleting any nodes, manually verify that they are indeed phantom duplicates and not legitimate isolated nodes. Do not blindly delete all unconnected nodes.

### Safe Cleanup Process

1. **Identify the affected query pattern** in your application:
   - Search your codebase for `CREATE ... SET`, `CREATE ... REMOVE`, `CREATE ... MERGE`, or `CREATE ... FOREACH` in the same query
   - Note the label(s) and properties of inline target nodes

2. **Audit the database**:
   - Run the detection query above for the suspected label
   - Manually inspect 10–20 results to confirm they are phantom duplicates
   - Compare an isolated node with a connected node; if they are identical except for connectivity, it is a phantom

3. **Delete confirmed phantoms** (after backup):
   ```cypher
   -- DELETE isolated nodes matching a specific label and property pattern
   MATCH (orphan:SuspectedLabel) WHERE NOT (orphan)--()
   DELETE orphan
   LIMIT 100
   RETURN count(orphan) AS deleted
   ```
   
   Run in batches (LIMIT 100) to avoid overwhelming the transaction log and to allow verification between runs.

4. **Verify repair**:
   - Re-run the detection query for that label
   - Confirm the orphan count decreases
   - Spot-check a few remaining nodes to ensure they are legitimate singletons

### Legitimate Unconnected Nodes

These are **not** phantom duplicates and should **not** be deleted:

- Nodes created with `CREATE (n:Label {...})` without any relationship
- Nodes from other write operations that don't create relationships
- Intentional graph structures with isolated nodes (e.g., a staging area)
- Nodes awaiting connection via later queries

The phantom-target bug **only affects** `CREATE` with **both** a relationship pattern **and** a write clause (`SET`, `REMOVE`, `MERGE`, or `FOREACH`) **in the same query**. If your application never uses that combination, your database is unaffected.

## Prevention

Upgrade to Nexus 3.0.0 or later. The fix ensures that:

1. Pattern elements in `CREATE` are materialized exactly once
2. Query variables are always bound to the connected node, not an orphan
3. `SET`, `MERGE`, `REMOVE`, and `FOREACH` in the same `CREATE` query operate on the correct nodes

If you are on an older version, audit your databases using the queries above and migrate to 3.0.0 to prevent future corruption.

## Status

- **Fixed**: Yes (verified by regression tests)
- **Scope**: Silent data corruption on specific query patterns
- **Remediation**: Detect, verify, and delete phantom orphans
- **Breaking Change**: No (correctness fix only)
