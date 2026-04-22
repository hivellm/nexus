# Geospatial Predicates & R-tree — Technical Design

## Scope

Add an R-tree index, spatial query predicates, and a `spatial.*`
procedure namespace. All work is additive to the existing `Point`
type and scalar `distance()` function.

## Index: packed Hilbert R-tree

### Why Hilbert R-tree

- Bulk-load is deterministic and disk-local (maps 2D/3D points to
  a 1D Hilbert curve, then sorts).
- Query performance competitive with R*-tree for read-heavy workloads.
- Nexus is read-heavy by design; R* split costs hurt on writes.

### Node layout (8 KB pages)

```
page_header:    32 bytes   (magic, version, level, flags, count)
children:       N × ChildRef
```

```rust
struct ChildRef {
    bbox: [f64; 4],        // min_x, min_y, max_x, max_y (or 6 bytes for 3D)
    child_ptr: u64,        // leaf: node id; internal: child page id
    _pad: u32,
}
```

Max fanout ≈ 127 children per page (8 KB / 64 B). Leaves store direct
node IDs, internal nodes store child page IDs.

### Bulk load

1. Extract `(node_id, point)` pairs via a label+property scan.
2. Compute Hilbert index for each point at a fixed precision (48-bit
   per dimension).
3. Sort by Hilbert index.
4. Pack bottom-up: 127 entries per leaf, build internal levels
   recursively.

Bulk-load cost: `O(n log n)` dominated by the sort. For 10M points:
< 30 s on a single CPU, < 4 GB peak RAM.

### Incremental insert / delete

- Insert: tree-descend choosing the child whose bbox expansion is
  minimal; split on overflow with quadratic split heuristic.
- Delete: locate + remove; on underflow, re-insert orphaned entries
  rather than merging (simpler, competitive for read-heavy).

### WAL + MVCC

R-tree mutations are WAL-journalled with the existing op codes:

- `OP_RTREE_INSERT { index_id, node_id, bbox }`
- `OP_RTREE_DELETE { index_id, node_id }`
- `OP_RTREE_BULKLOAD_DONE { index_id, root_page }`

Reads follow the existing MVCC pattern: the R-tree is a *projection*
of the primary store, so readers at snapshot epoch `E` must ignore
entries whose owning node is either not yet committed at `E` or
already tombstoned. Implementation: a visibility filter is applied
after the R-tree seek but before emitting rows.

### Concurrency

Writers serialise on the per-index mutex (same as B-tree, bitmap).
Readers are lock-free. During bulk rebuild, the old R-tree stays
queryable; once the new one is built and WAL-synced, an atomic
pointer swap promotes it. This preserves snapshot isolation without
readers blocking.

## Planner integration

New operator `SpatialSeek`:

```rust
struct SpatialSeek {
    index_id: IndexId,
    mode: SeekMode,
    limit: Option<usize>,
}

enum SeekMode {
    Bbox(BBox),
    WithinDistance { center: Point, meters: f64 },
    Nearest { point: Point, k: usize },
}
```

Predicates recognised by the planner as seekable:

| Cypher fragment                                           | Seek mode                        |
|-----------------------------------------------------------|----------------------------------|
| `WHERE point.withinBBox(n.loc, $bbox)`                    | `Bbox(bbox)`                     |
| `WHERE point.withinDistance(n.loc, $p, $d)`               | `WithinDistance { p, d }`        |
| `ORDER BY distance(n.loc, $p) LIMIT $k`                   | `Nearest { p, k }`               |
| `WHERE point.nearest(n.loc, $k)` (returns LIST<NODE>)     | direct seek; bypasses MATCH scan |

Cost model: R-tree seek cost ≈ `log_b(n) + matching_entries`, where
`b` is fanout (127). Falls back to label scan + filter if the estimate
is worse than scanning.

## Function surface

```
point.withinBBox(p: POINT, bbox: MAP) -> BOOLEAN
point.withinDistance(a: POINT, b: POINT, distMeters: FLOAT) -> BOOLEAN
point.azimuth(a: POINT, b: POINT) -> FLOAT
point.nearest(p: POINT, k: INTEGER) -> LIST<NODE>
```

`bbox` is a map `{bottomLeft: POINT, topRight: POINT}` (Neo4j format).

## Procedure surface

```
spatial.bbox(points: LIST<POINT>) -> {bottomLeft: POINT, topRight: POINT}
spatial.distance(a: POINT, b: POINT) -> {meters: FLOAT}
spatial.nearest(p: POINT, label: STRING, k: INTEGER) -> (node:NODE, dist:FLOAT)*
spatial.interpolate(line: LIST<POINT>, frac: FLOAT) -> POINT
```

`spatial.nearest` streams rows ordered by distance ascending.

## CRS handling

Supported CRS (matching Neo4j):

| Name             | srid  | Dimensions | Distance metric      |
|------------------|-------|------------|----------------------|
| Cartesian-2D     | 7203  | 2          | Euclidean            |
| Cartesian-3D     | 9157  | 3          | Euclidean            |
| WGS-84-2D        | 4326  | 2          | Haversine (meters)   |
| WGS-84-3D        | 4979  | 3          | Haversine + Δh       |

Mixed-CRS operations fail with `ERR_CRS_MISMATCH(a_srid, b_srid)`.

## DDL grammar

```
CREATE INDEX [index_name]
  FOR (n:Label) ON (n.prop)
  USING RTREE
  [OPTIONS { dimensions: 2|3, crs: 'wgs84'|'cartesian' }]
```

If `OPTIONS` is omitted, the index infers dimensions and CRS from
the first inserted point.

## Error taxonomy

| Code                  | Raised when                                      |
|-----------------------|--------------------------------------------------|
| `ERR_CRS_MISMATCH`    | Mixed CRS in a distance or predicate              |
| `ERR_BBOX_MALFORMED`  | bbox map missing required keys or bad types      |
| `ERR_DIM_MISMATCH`    | 2D point compared to 3D point                    |
| `ERR_RTREE_BUILD`     | Bulk-load target property has non-point values   |

## Benchmarks (targets)

| Scenario                           | Target p95  |
|------------------------------------|-------------|
| `withinDistance` 1 M points, 100 m radius | < 3 ms     |
| `nearest` k=10 over 1 M points      | < 2 ms      |
| Bulk-load 10 M points               | < 30 s      |
| Insert 10 k writes/sec sustained    | 10 k ops/s  |

## Rollout

- v1.3.0 ships R-tree + spatial predicates.
- R-tree indexes are opt-in via `USING RTREE`; no existing data is
  migrated.
- `db.indexes()` reports R-tree indexes with `type = "RTREE"`.
