# Proposal: phase8_quantified-path-patterns-execution

## Why

Cypher 25 / Neo4j 5.9+ introduced quantified path patterns: `(a)-[:KNOWS]->{1,5}(b)`, `((a)-[:NEXT]->(b)){2,}`, etc. Grammar + AST landed in `phase6_opencypher-quantified-path-patterns` (2026-04-21) but the executor side is unfinished — the engine parses the syntax and either errors or falls through to the legacy `*1..5` variable-length-path operator without honouring the new semantics (group quantification, mode `WALK | TRAIL | ACYCLIC | SIMPLE`). Without this Nexus cannot run a meaningful subset of Neo4j-5.9+ workloads even though it claims Cypher 25 in the README.

## What Changes

- Implement a `QuantifiedExpand` physical operator: BFS / DFS expansion bounded by `{lo,hi}` with mode awareness (WALK = revisits OK, TRAIL = no edge revisits, ACYCLIC = no node revisits, SIMPLE = TRAIL + ACYCLIC).
- Wire the operator into the planner: pattern groups `((...)-[...]->(...)){n,m}` become a single QuantifiedExpand that yields one row per matching path, carrying the path variable through projection.
- Implement `path_function(p)` accessors (length, nodes, relationships) on the produced paths.
- Add per-shape executor tests covering all 4 modes × edge cases (zero-length match, unbounded upper, named-vs-unnamed groups).
- Run Neo4j diff-suite to confirm 300/300 still passes; expand suite if Neo4j-5.9+ scenarios surface.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (mark quantified paths as supported).
- Affected code: `crates/nexus-core/src/executor/operators/expand.rs` (new QuantifiedExpand variant), `crates/nexus-core/src/executor/planner/`.
- Breaking change: NO (currently errors → now executes).
- User benefit: Neo4j 5.9+ Cypher parity; existing apps using quantified-path syntax run on Nexus.
