# Clean Graph Rebuild & the Null-Key Contract

## Why
Legacy nodes ingested before the 2.3.x fixes carry `null` `id` / `name` property values. A node addressed by an index seek such as `MATCH (n:Label {id: $v})` can never match a null-keyed node (null is "unknown"), and historically such null values polluted the typed property index and label scans. Nexus 2.3.1 enforces a null-key contract and supports a deterministic clean rebuild so downstream clients (e.g. the Cortex bootstrap worker) can re-ingest cleanly.

## The null-key contract (Neo4j-aligned)
Nexus follows Neo4j semantics: a property whose value is `null` is treated as **absent**.

1. **Null values are never indexed.** Writing a property with value `null` does not create an entry in the typed property index. Therefore an index seek for a null value (`find_exact(label, key, null)`) always returns nothing, and null-valued properties cannot pollute index seeks or be addressed by `MATCH (n:Label {key: $v})`.
2. **MERGE rejects null keys.** `MERGE (n:Label {id: null})` fails with a runtime error: `Cannot merge node using null property value for id` — identical to Neo4j. Resolve null-valued keys (or omit them) before issuing a MERGE.

Practical consequence: re-ingesting with the fixes (parameters no longer collapsing to null, MERGE rejecting null keys) yields a graph with no null-keyed nodes, so index seeks address every node deterministically.

## Clean rebuild procedure
Use this to wipe a polluted graph and re-bootstrap deterministically. The actual re-ingest is driven by the downstream client; Nexus provides the drop/recreate primitives.

1. **Drop the database** (removes all node/relationship stores, catalog, and indexes):
   ```cypher
   DROP DATABASE mydb IF EXISTS
   ```
   or via REST: `DELETE /databases/mydb`.
2. **Recreate the database:**
   ```cypher
   CREATE DATABASE mydb
   ```
   or `POST /databases` with `{"name": "mydb"}`.
3. **Recreate indexes** for the labels/properties you seek on, e.g.:
   ```cypher
   CREATE INDEX FOR (n:Label) ON (n.id)
   ```
4. **Re-ingest** with the fixed client. Because null values are not indexed and MERGE rejects null keys, the rebuilt graph contains only addressable, non-null keys.

Indexes are rebuilt as data is ingested; the catalog (label / type / key mappings) is reconstructed from the freshly written records. No manual index repopulation step is required beyond declaring the indexes you want.

> Note on Windows: directory deletion during `DROP DATABASE` retries with backoff; the database is always removed from the manager even if the on-disk directory lingers briefly.

## Sequencing
This ships after the read-side index-seek and `parameters: null` fixes (issues #7, #8). The downstream graph worker runs the rebuild once those land.
