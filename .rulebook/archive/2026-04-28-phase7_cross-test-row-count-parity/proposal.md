# Proposal: phase7_cross-test-row-count-parity

## Why

The 74-test cross-bench against Neo4j (`docs/performance/BENCHMARK_NEXUS_VS_NEO4J.md`) reports 73/74 Nexus-faster but only **52/74 row-identical**. The 22 incompatible tests are not correctness bugs — they are projection-semantics differences:

- `OPTIONAL MATCH` returns NULL row when nothing matches; Neo4j wraps in one NULL row, Nexus returns zero rows on some shapes.
- `WITH` projection w/ chained projection — Neo4j carries hidden grouping; Nexus collapses earlier.
- `Write` operations — Neo4j returns implicit `success=true` row; Nexus returns affected ids.
- `ORDER BY` / `DISTINCT` — same answer, different implicit ordering on ties.

These differences make automated migration scripts and existing Neo4j-targeted tooling fail on identical-data outputs. Closing the gap takes cross-test compatibility from 70 % to ~95 % at low engineering cost (~1 week).

## What Changes

- Audit the 22 incompatible tests and classify each by root cause (4 categories above plus any outliers).
- For each category, adjust the executor's projection / row-emission semantics to match Neo4j's exact behavior, behind a flag `neo4j_strict_rows` (default ON).
- Add per-category regression tests pinning the new behaviour.
- Re-run the 74-test bench and confirm ≥ 70/74 row-identical (target 73/74).
- Verify Neo4j diff-suite still passes 300/300.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` projection semantics section.
- Affected code: `crates/nexus-core/src/executor/operators/{project,optional_match,write}.rs`.
- Breaking change: small — clients depending on exact current row shapes for OPTIONAL/WITH/Write may see one extra wrapper row. Document in CHANGELOG.
- User benefit: drop-in compatibility with Neo4j-targeted client code; cleaner marketing.
