# Proposal: phase8_optional-match-binding-leak

Source: carved out of `phase7_cross-test-row-count-parity` after a
probe found this is the second distinct OPTIONAL MATCH bug. Higher
severity than the parent's "row-count parity" framing — this one
returns **wrong data**, not just an extra/missing row.

## Why

Probe `cargo +nightly test -p nexus-core --test
zzz_probe_optional_match` (deleted after the audit, output captured
in `phase7_cross-test-row-count-parity` archive) showed:

```cypher
CREATE (:Person {name: 'Alice'});

MATCH (a:Person)
OPTIONAL MATCH (a)-[:KNOWS]->(b:Person)
RETURN a.name AS name, b.name AS friend
-- Nexus today returns: ['Alice', 'Alice']
-- Neo4j expects:        ['Alice', null]
```

Alice has no `:KNOWS` outgoing edge, so `b` should be NULL and
`b.name` should be NULL. Nexus instead binds `b` to `a` itself and
returns `b.name = 'Alice'`. The same shape returns the full Alice
node object when `b` is projected directly:
`['Alice', {_nexus_id: 0, name: 'Alice'}]`.

This is a **correctness bug**, not a parity issue: the user gets
silent wrong data on every OPTIONAL MATCH that fails to match,
which corrupts every analytic query that uses `OPTIONAL MATCH +
COALESCE(...)` or `OPTIONAL MATCH + COUNT(b)` to detect missing
relationships. Aggregations + subqueries built on top inherit the
corruption.

## What Changes

- `crates/nexus-core/src/executor/operators/expand.rs:374-387`
  already has a "LEFT OUTER JOIN preserve row with NULL values
  when optional=true" branch. The bug is that whatever the
  surrounding driver-row carries for the target variable is **not
  cleared** before the projection sees it; the prior MATCH's `a`
  binding leaks into `b`'s slot.
- Investigate: when Expand emits the no-match LEFT-OUTER row, does
  it actively `set_variable(target_var, Value::Null)` and
  `set_variable(rel_var, Value::Null)`, or does it leave the
  surrounding scope untouched (which is what causes the leak)?
  The fix is likely a 5-line change in expand.rs:374 to bind both
  variables explicitly to NULL on the no-match path.
- Tests: pin the contract end-to-end against `Engine::execute_cypher`:
  - `MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) RETURN
    a.name AS name, b` → b is NULL.
  - `MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b:Person)
    RETURN a.name AS name, b.name AS friend` → friend is NULL.
  - `MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b)
    RETURN a, COALESCE(b.name, 'anon') AS friend` → friend = 'anon'.
  - Aggregation: `MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b)
    RETURN count(b) AS friends` → friends = 0.
- Add proptest covering: for every node with no outgoing edges,
  OPTIONAL MATCH binds rel + target to NULL, NEVER to the source.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` OPTIONAL MATCH
  semantics section needs a "no-match path binds variables to
  NULL, not to the source row" note.
- Affected code:
  `crates/nexus-core/src/executor/operators/expand.rs:374-387`
  (and possibly :228-240 for the chained-OPTIONAL no-source
  path), executor context layer.
- Breaking change: small — clients relying on the bug to access
  the source variable through the target slot will see NULL
  instead. Likely zero such clients exist (the behaviour is
  silently wrong; nobody depends on it on purpose).
- User benefit: removes silent data corruption from every
  OPTIONAL MATCH no-match path. Restores the contract every
  Neo4j-targeted analytic query already assumes.

## Source

- Parent task: `phase7_cross-test-row-count-parity`.
- Sibling: `phase8_optional-match-empty-driver` covers the
  standalone-OPTIONAL-MATCH-zero-row bug.
- Severity: **high** — silent wrong data, not a row-count delta.
  Should ship before any task that builds analytics on top of
  OPTIONAL MATCH.
