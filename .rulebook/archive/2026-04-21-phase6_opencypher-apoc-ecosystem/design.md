# APOC Ecosystem — Technical Design

## Scope

Deliver an APOC-compatible procedure library covering ~200
procedures across 9 namespaces. "Compatible" means the procedure
name, argument types, return column names, and observable behaviour
match Neo4j's APOC for the supported surface. 100% parity is not the
goal; 95% of real-world usage is.

## Crate layout

```
nexus-apoc/
├── Cargo.toml
└── src/
    ├── lib.rs          -- public register_all(registry: &mut Registry)
    ├── coll/           -- 30 procedures
    ├── map/            -- 20 procedures
    ├── date/           -- 25 procedures
    ├── text/           -- 20 procedures
    ├── path/           -- 25 procedures
    ├── periodic/       -- 5 procedures
    ├── load/           -- 8 procedures
    ├── schema/         -- 10 procedures
    └── export/         -- 10 procedures
```

`register_all` is called by `nexus-server` at startup. If the server
is built without the apoc feature, the crate is omitted and `apoc.*`
calls fail with `ERR_PROC_NOT_FOUND` (standard behaviour).

## Procedure shape

Each procedure implements the same `SystemProc` trait used by
`db.*` / `dbms.*`:

```rust
impl SystemProc for CollUnion {
    fn name(&self) -> &'static str { "apoc.coll.union" }
    fn signature(&self) -> ProcSignature { /* list<any>, list<any> -> list<any> */ }
    fn mode(&self) -> ProcMode { ProcMode::Read }
    fn min_role(&self) -> Role { Role::Reader }
    fn call(&self, ctx: &CallCtx, args: Vec<Value>) -> Result<RowStream> { ... }
}
```

## Neo4j compatibility matrix

We ship a compatibility doc (`APOC_COMPATIBILITY.md`) that lists for
every procedure:
- Name.
- Signature match (exact / equivalent / subset).
- Known deviations (if any).
- Link to the corresponding Neo4j APOC doc page.

Procedures whose behaviour would require enterprise-only features
(GDS's Pregel, APOC Extended's ML models) are skipped with a clear
note.

## apoc.periodic.iterate

```
apoc.periodic.iterate(
    driveQuery: STRING,     -- produces rows
    actionQuery: STRING,    -- executed per row
    config: MAP             -- { batchSize, parallel, concurrency, retries, ... }
)
```

Implementation: desugars to `CALL { ... } IN TRANSACTIONS` under the
hood. Output columns match Neo4j's: `batches, total, timeTaken,
committedOperations, failedOperations, failedBatches,
retries, errorMessages, batch, operations, wasTerminated, failedParams`.

Because we depend on the subquery-transactions task, this procedure
is purely a rewriter + stats collector; no new executor internals.

## apoc.load.* safety

APOC's load procedures are the biggest security footgun in the
Neo4j ecosystem (they can make arbitrary HTTP or file reads at the
database layer). Our sandboxing:

| Config key                     | Default | Effect                                                                 |
|--------------------------------|---------|------------------------------------------------------------------------|
| `apoc.import.file.enabled`     | `false` | When false, `file:` URLs in apoc.load.* fail with `ERR_IMPORT_DISABLED`|
| `apoc.http.enabled`            | `true`  | When false, all HTTP loads fail                                        |
| `apoc.http.allow`              | (empty) | Regex allow-list; empty means all hosts allowed                        |
| `apoc.http.timeout_ms`         | 5000    | Per-request timeout                                                    |
| `apoc.export.file.enabled`     | `false` | When false, `apoc.export.*` file writes fail                           |
| `apoc.export.base_dir`         | `<data_dir>/exports/` | Writes are rooted here; path traversal rejected                |

All are loaded from the server config file. Startup logs the
effective values; disabling `apoc.http.enabled` in prod is the
recommended hardened configuration.

## apoc.path.* configuration

`apoc.path.expand(config)` accepts:

```
{
    relationshipFilter:  "TYPE1|TYPE2",   -- OR: "TYPE1>TYPE2<"  with direction
    labelFilter:         "+INCLUDE|-EXCLUDE|/TERMINATOR",
    minLevel:            1,
    maxLevel:            -1,                -- -1 = unbounded
    uniqueness:          "RELATIONSHIP_PATH"|"NODE_PATH"|"NODE_GLOBAL"|"RELATIONSHIP_GLOBAL",
    bfs:                 true,
    limit:               -1
}
```

Compiler translates this to the existing Nexus traversal operators
(ExpandInto / ExpandAll / PathSelect) with the appropriate filter
pushdown. `uniqueness` is enforced by per-frame bitmaps identical to
the QPP task's cycle-policy implementation (shared module).

## apoc.coll.* hot paths

We target C-speed on the hot list ops: `flatten`, `sort`, `union`,
`intersection`. Implementation uses `SmallVec<[Value; 8]>` to avoid
heap allocation for small lists. Collections > 1M items fall back to
`Vec`.

## Benchmarks

| Procedure                                   | Target p95              |
|---------------------------------------------|-------------------------|
| `apoc.coll.flatten([[...1k...], [...1k...]])` | < 1 ms                |
| `apoc.coll.sort` on 100k integers           | < 15 ms                 |
| `apoc.periodic.iterate` 1M rows, batch 1k   | ≥ 20k rows/sec/thread   |
| `apoc.path.expand` 5 hops on 10k-node graph | < 20 ms                 |
| `apoc.date.format` single call              | < 10 µs                 |

## Coverage matrix

`docs/procedures/APOC_COMPATIBILITY.md` tracks:

```
| Procedure                    | Shipped | Parity | Notes                |
|------------------------------|---------|--------|----------------------|
| apoc.coll.union              | ✅      | exact   | -                    |
| apoc.coll.intersection       | ✅      | exact   | -                    |
...
```

CI gates on "parity % per release ≥ last release's value", so we
cannot accidentally regress coverage.

## Out of scope for v1

- `apoc.cypher.*` — runs dynamic Cypher; deferred to a later release
  as its own task due to security implications.
- `apoc.ml.*` — ML model integrations.
- `apoc.jdbc.*` — needs a JDBC connector; separate task.
- `apoc.trigger.*` — requires write triggers in the kernel.
- `apoc.bolt.*` — needs Bolt wire protocol (roadmap V2).

## Rollout

- The APOC ecosystem ships in two releases to amortise review load:
  - **v1.4.0**: coll, map, date, text, periodic, schema (core).
  - **v1.5.0**: path, load, export (external surfaces).
- Feature flag `apoc_enabled = true` by default; users can disable
  at startup to strip the crate from the running binary.
- APOC compatibility matrix updated per release.
