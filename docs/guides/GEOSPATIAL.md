# Geospatial Guide

End-user guide for spatial data in Nexus. Covers the
`point.*` predicates, `spatial.*` procedures, and the
`CREATE INDEX … USING RTREE` DDL.

For internals (page layout, bulk-load algorithm, WAL framing)
see [`docs/specs/rtree-index.md`](../specs/rtree-index.md).

## Quick start

Insert nodes with point properties, create an R-tree index,
run a k-NN query.

```cypher
// 1. Insert two stores with locations.
CREATE (:Store {name: 'Alpha', loc: point({x: 0.0, y: 0.0})});
CREATE (:Store {name: 'Beta',  loc: point({x: 5.0, y: 5.0})});

// 2. Create a spatial index. Both shapes register the same
//    `IndexManager::rtree` entry; pick whichever your script
//    style prefers.
CREATE SPATIAL INDEX ON :Store(loc);
//   ── or ──
CREATE INDEX store_loc FOR (s:Store) ON (s.loc) USING RTREE;

// 3. Find the 3 stores closest to a point.
CALL spatial.nearest(point({x: 1.0, y: 1.0}), 'Store', 3)
YIELD node, dist
RETURN node, dist;
```

## Coordinate Reference Systems

Each `point()` carries a CRS hint. v1 supports:

| CRS         | Behaviour                                     |
|-------------|-----------------------------------------------|
| `cartesian` | 2-D Euclidean. `point.distance` returns sqrt(dx² + dy²). |
| `wgs-84`    | Reserved for great-circle distance. Currently unsupported in `spatial.nearest` / `point.withinDistance` — calls return a typed `WGS-84 metric is not implemented yet (use Metric::Cartesian)` error. |

Mixing CRSes in a single index is not supported; the executor
filters out points whose CRS differs from the query point so a
mis-tagged write does not silently distort the result.

## Predicates

### `point.withinBBox(p, lower, upper)`

`true` when `p` lies in the closed bounding box `[lower.x, upper.x]
× [lower.y, upper.y]`.

```cypher
MATCH (s:Store)
WHERE point.withinBBox(
        s.loc,
        point({x: 0, y: 0}),
        point({x: 10, y: 10}))
RETURN s.name;
```

### `point.withinDistance(p, q, max)`

`true` when the Cartesian distance between `p` and `q` is at most
`max`. Uses the R-tree's `within_distance` walk, pruning by
squared bbox distance.

```cypher
MATCH (s:Store)
WHERE point.withinDistance(s.loc, point({x: 1, y: 1}), 2.5)
RETURN s.name;
```

### `point.distance(p, q)`

Returns the Cartesian distance between `p` and `q`. Does not use
the index — strictly a scalar function.

## Procedures

### `CALL spatial.nearest(p, label, k)`

Returns the `k` nearest `:label` nodes ordered by ascending
distance. Ties break on `node_id` ascending so the result is
deterministic across runs.

```cypher
CALL spatial.nearest(point({x: 0, y: 0}), 'Store', 5)
YIELD node, dist
RETURN node.name AS name, dist;
```

Internally:

1. Resolve the `{label}.<prop>` index from the registry.
2. Walk the packed Hilbert R-tree's k-NN priority queue
   (O(log_b N + k) page reads).
3. Drop entries whose CRS does not match `p`.

### `CALL spatial.addPoint(node_id, prop, point)`

Indexes `point` for `node_id` under the `{label}.{prop}` index.
Used by ingest pipelines that materialise spatial state outside
the regular CREATE / SET path.

## DDL

### `CREATE SPATIAL INDEX ON :Label(prop)`

Legacy form; registers an R-tree on `IndexManager::rtree` keyed
`{Label}.{prop}`.

### `CREATE INDEX [name] FOR (n:Label) ON (n.prop) USING RTREE`

Cypher 25 / Neo4j-dialect form. Equivalent to the legacy form;
the optional name lets `db.indexes()` and `DROP INDEX <name>`
target the index by a stable handle.

### `DROP INDEX <name>`

Drops the R-tree from the registry. The on-disk file (if any)
is unlinked.

## Performance

| Scenario                                  | Target                   |
|-------------------------------------------|--------------------------|
| 1 M-point `nearest(k = 10)` p95           | < 2 ms                   |
| 1 M-point `withinDistance` p95            | < 3 ms                   |
| 10 M-point bulk-load                      | < 30 s                   |
| Sustained insert throughput               | ≥ 10 k writes/sec        |

The packed Hilbert R-tree replaces the earlier grid-backed
prototype which scaled `O(N)` for any nearest-neighbour query.
With the new backend, `spatial.nearest(k = 10)` over 1 M points
budgets log₁₂₇(1 M) ≈ 3 page reads + 10 leaf pops on the hot
path.

## Tuning knobs

`ExecutorConfig` exposes:

- `rtree_default_metric` — `Cartesian` (default) or `Wgs84`
  (placeholder, returns the typed error documented above).

Engine-wide knobs in `nexus.toml` (slated for a follow-up
config-loading pass):

- `nexus.spatial.default_batch_size` — bulk-load chunk size.
- `nexus.spatial.flush_interval_ms` — interval between
  `PageStore::flush` calls when the engine is buffering writes.

## Crash recovery

R-tree mutations land in the WAL with three op-codes
(`RTreeInsert`, `RTreeDelete`, `RTreeBulkLoadDone`). On engine
startup the recovery loop walks the WAL once and feeds every
entry through `RTreeRegistry::apply_wal_entry`. The in-memory
tree converges back to the durable shape; partial bulk-loads
(missing the `RTreeBulkLoadDone` marker) are detected and
re-run.

The integration test
`crates/nexus-core/tests/rtree_crash_recovery.rs` exercises
the full path: 5 000 committed inserts + a 500-row partial
bulk-load → drop the registry → replay → assert every point
is reachable through `query_bbox` / `within_distance`.

## Limitations

- WGS-84 great-circle distance is not yet implemented.
- 3-D points round-trip through the page codec but the
  query operators ignore the z-coord today.
- Concurrent bulk-rebuild via `RTreeRegistry::swap_in` is
  available; the parser-side DDL surface to trigger it
  (`REINDEX`) lands with the auto-populate slice
  (`phase6_spatial-index-autopopulate`).
- `spatial.knn` currently uses Cartesian distance only; once
  the WGS-84 helpers land, the procedure will accept a CRS
  argument matching `point.distance`.
