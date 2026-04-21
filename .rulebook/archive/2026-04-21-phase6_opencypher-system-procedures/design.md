# System Procedures — Technical Design

## Scope

Ship Neo4j's stable `db.*` and `dbms.*` procedure surface. This covers
~30 procedures across five namespaces. All procedures are read-only
introspection over data that already exists in the catalog or in-memory
registries, so the work is wiring, not new subsystems.

## Procedure inventory

| Namespace      | Procedure                          | Columns                                                | Source of data                       |
|----------------|------------------------------------|--------------------------------------------------------|--------------------------------------|
| `db.schema`    | `visualization()`                  | nodes:LIST<NODE>, relationships:LIST<REL>              | Catalog + storage sampler            |
| `db.schema`    | `nodeTypeProperties()`             | nodeType, nodeLabels, propertyName, propertyTypes, mandatory | Catalog property sampler       |
| `db.schema`    | `relTypeProperties()`              | relType, propertyName, propertyTypes, mandatory        | Catalog property sampler             |
| `db`           | `labels()`                         | label:STRING                                           | Catalog                              |
| `db`           | `relationshipTypes()`              | relationshipType:STRING                                | Catalog                              |
| `db`           | `propertyKeys()`                   | propertyKey:STRING                                     | Catalog                              |
| `db`           | `indexes()`                        | id, name, state, populationPercent, uniqueness, type, entityType, labelsOrTypes, properties, indexProvider | Index manager |
| `db`           | `indexDetails(name)`               | same as indexes() but single-row                       | Index manager                        |
| `db`           | `constraints()`                    | id, name, type, entityType, labelsOrTypes, properties, ownedIndex | Constraint manager        |
| `db`           | `info()`                           | id, name, creationDate                                 | Multi-DB registry                    |
| `db.stats`     | `retrieve('GRAPH COUNTS')`         | section, data                                          | Counts store                         |
| `dbms`         | `components()`                     | name, versions:LIST<STRING>, edition:STRING            | Static + build info                  |
| `dbms`         | `procedures()`                     | name, signature, description, mode, worksOnSystem      | Procedure registry self-scan         |
| `dbms`         | `functions()`                      | name, signature, description, aggregating              | Function registry self-scan          |
| `dbms`         | `info()`                           | id, name, creationDate                                 | Server info                          |
| `dbms`         | `listConfig(search)`               | name, description, value, dynamic                      | Config loader                        |
| `dbms`         | `showCurrentUser()`                | username, roles:LIST<STRING>, flags:LIST<STRING>       | Auth session                         |
| `dbms`         | `cluster.routing.getRoutingTable()`| ttl, servers                                           | Cluster state (cluster-mode only)    |

## Registry model

```rust
// nexus-core/src/procedures/system/mod.rs
pub trait SystemProc: Send + Sync {
    fn name(&self) -> &'static str;
    fn signature(&self) -> ProcSignature;
    fn mode(&self) -> ProcMode;                // READ / WRITE / DBMS
    fn min_role(&self) -> Role;                // Reader / Editor / Admin
    fn call(&self, ctx: &CallCtx, args: Vec<Value>) -> Result<RowStream>;
}

pub struct SystemProcRegistry {
    by_name: HashMap<&'static str, Arc<dyn SystemProc>>,
}

impl SystemProcRegistry {
    pub fn default() -> Self { /* registers all built-ins */ }
}
```

Dispatch happens in `executor/operators/procedures.rs`:

```rust
match proc_name.as_str() {
    n if n.starts_with("gds.")  => gds_registry.lookup(n)?,
    n if n.starts_with("db.")   => system_registry.lookup(n)?,
    n if n.starts_with("dbms.") => system_registry.lookup(n)?,
    n if n.starts_with("apoc.") => apoc_registry.lookup(n)?,   // Phase 6g
    _                           => return Err(ProcNotFound),
}
```

## Row streaming

All procedures return `RowStream` which is `Stream<Item = Vec<Value>>`.
For small result sets (< 1000 rows) we materialise eagerly; for
`db.schema.visualization` on large graphs we stream in 512-row pages.

Columns are typed exactly to match Neo4j so drivers deserialise
without surprise:

```rust
ProcSignature {
    inputs: vec![Arg { name: "name", ty: Type::String, optional: true }],
    outputs: vec![
        Column { name: "nodes",         ty: Type::List(Box::new(Type::Node)) },
        Column { name: "relationships", ty: Type::List(Box::new(Type::Relationship)) },
    ],
}
```

## Authorisation

Uses the existing RBAC engine. Each procedure declares a minimum role:

| Mode    | Minimum role | Procedures                                        |
|---------|--------------|---------------------------------------------------|
| READ    | Reader       | Everything under `db.*` except config             |
| DBMS    | Admin        | `dbms.listConfig`, `dbms.cluster.*`               |

A caller with `Reader` attempting `dbms.listConfig` receives HTTP 403
with code `ERR_PERMISSION_DENIED` — no partial results.

## Multi-database scoping

Every `db.*` procedure runs in the context of the caller's current
session database. Cross-db peeking MUST NOT happen. The `CallCtx`
passed to `call()` holds the resolved `DatabaseId`; procedures go
through `ctx.catalog()` and never directly through the global
multi-db registry.

## Sampling vs exhaustive scans

`db.schema.nodeTypeProperties` in Neo4j samples up to
`db.schema.samplingSize` (default 1000) nodes per label. We adopt the
same default and expose the knob as `nexus.schema.samplingSize` in
config. Exhaustive mode is available via
`CALL db.schema.nodeTypeProperties({sample: 0})` (0 = unbounded).

## `dbms.procedures()` self-description

Every SystemProc reflects its own signature at registration time, so
`dbms.procedures()` is a registry walk that emits metadata rows.
APOC and GDS registries register with the same system registry so
they appear in the catalogue too.

## Performance budget

| Procedure                         | Target p95 latency | Notes                                 |
|-----------------------------------|---------------------|--------------------------------------|
| `db.labels()`                     | < 1 ms              | Reads from catalog bitmap            |
| `db.indexes()`                    | < 5 ms              | Reads in-memory index manager        |
| `db.schema.visualization()`       | < 200 ms @ 10k nodes| One sampling pass                    |
| `db.schema.nodeTypeProperties()`  | < 500 ms @ 100k nodes per label | Bounded by sampling size |
| `dbms.procedures()`               | < 10 ms             | Registry walk                        |

## Wire format

All procedures return results through the existing `/cypher`
endpoint, so no new HTTP surface is added. The response body follows
the standard shape:

```json
{
  "columns": ["label"],
  "rows": [["Person"], ["Movie"]],
  "execution_time_ms": 1
}
```

## Rollout

Ships in release `v1.2.0` alongside APOC Phase 6g prerequisites. No
feature flag — additive change.

## Out of scope

- `db.indexes.fulltext.*` — covered by the full-text task.
- `apoc.*` — covered by the APOC task.
- `db.migrations.*` — not a Neo4j stable namespace.
- Enterprise-only procedures (`dbms.cluster.overview` beyond routing).
