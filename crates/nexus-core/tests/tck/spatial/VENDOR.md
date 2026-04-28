# Nexus Spatial TCK Corpus

## Origin

This directory holds the spatial Cypher conformance suite as Gherkin
`.feature` files in the openCypher TCK shape. Unlike the rest of the
openCypher TCK (which Nexus could vendor verbatim from
`https://github.com/opencypher/openCypher`), the **upstream openCypher
TCK ships no spatial scenarios**.

**Verified 2026-04-28** against `opencypher/openCypher@main`
(`tck/features/` tree). Coverage at that commit:

```
clauses/    {call, create, delete, match, match-where, merge, remove,
             return, return-orderby, return-skip-limit, set, union,
             unwind, with, with-orderBy, with-skip-limit, with-where}
expressions/{aggregation, boolean, comparison, conditional,
             existentialSubqueries, graph, list, literals, map,
             mathematical, null, path, pattern, precedence, quantifier,
             string, temporal, typeConversion}
useCases/   {countingSubgraphMatches, triadicSelection}
```

No `point`, `spatial`, `distance`, or `geographic` directories — at
any nesting level — exist in the upstream tree. Spatial Cypher was
historically a Neo4j extension and remains absent from the public
openCypher TCK as of the verification date.

## What this corpus is

Nexus-authored `.feature` files following the openCypher TCK
conventions (Background-Scenario-Given-When-Then, table-shaped
expectations, `no side effects` discipline) for the spatial Cypher
surface that Nexus itself ships:

- `point({x, y, z?, crs})` and `point({longitude, latitude, height?})`
  constructors
- `point.distance(p, q)` and `distance(p, q)`
- `point.withinBBox(p, {bottomLeft, topRight})`
- `point.withinDistance(p, q, d)`
- `point.nearest(<var>.<prop>, <k>)` (function-style projection)
- Coordinate accessors `p.x`, `p.y`, `p.z`, `p.longitude`,
  `p.latitude`, `p.height`, `p.srid`, `p.crs`
- `CREATE SPATIAL INDEX` / `DROP SPATIAL INDEX`
- `db.indexes()` RTREE rows
- `spatial.nearest()` / `spatial.addPoint()` procedures
- Planner `SpatialSeek` rewriter (Bbox / WithinDistance / Nearest)
- CRS-mismatch error path (`ERR_CRS_MISMATCH`)

## License

Apache 2.0 (matches upstream openCypher TCK + Nexus core). See
`LICENSE-NOTICE.md` at the repo root for the full attribution.

These files are authored under Apache 2.0 with no external
copyright; they are eligible for upstream contribution to
`opencypher/openCypher` if the openCypher Implementers Group ever
opens a spatial track.

## Reproduction

To re-verify "no upstream spatial corpus":

```bash
curl -fsSL "https://api.github.com/repos/opencypher/openCypher/git/trees/main?recursive=1" \
  | tr ',' '\n' \
  | grep -iE 'spatial|point|distance|geographic' \
  | head
```

Empty output = upstream still has no spatial scenarios. Update this
file's "Verified" date when you re-confirm.

## Running

```bash
cargo +nightly test -p nexus-core --test tck_runner --all-features
```

The harness lives in `crates/nexus-core/tests/tck_runner.rs` and uses
`cucumber 0.21`. Each scenario gets an isolated `Engine` (fresh
tempdir) so scenarios cannot leak state into each other.
