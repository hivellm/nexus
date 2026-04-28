# Proposal: phase7_kuzu-migration-guide

## Why

Kùzu Inc. archived the KuzuDB GitHub repo on 2025-10-10. The displaced user base — embedded analytics / GraphRAG / Cypher-with-WASM — is exactly Nexus's target quadrant. Multiple forks exist (Bighorn / Ladybug / RyuGraph) but all are early; FalkorDB has already published its own "Kuzu → FalkorDB migration" guide. Nexus has 12 months of opportunity before the forks consolidate or FalkorDB captures the majority. A clear migration guide is the lowest-effort, highest-leverage way to attract those users now.

## What Changes

- New doc `docs/migration/FROM_KUZU.md` covering: schema mapping, Cypher-dialect differences (Kuzu subset → Nexus 300/300 Neo4j-compat surface), data-loading from Kuzu's `.kz` files (or its CSV/Parquet exports), embedded-mode replacement story (Nexus is single-binary CLI + RPC vs Kuzu in-proc), vector-index migration, FTS migration, performance expectations, gotchas.
- Sample migration script under `scripts/migration/from_kuzu.py` that converts a Kuzu COPY-FROM CSV dump into a Nexus `LOAD CSV` + bulk RPC ingest.
- Cookbook at the end with three end-to-end examples: GraphRAG pipeline, recommendation system, knowledge graph.
- Cross-link from README (a "migrating from Kuzu" callout) and from the next CHANGELOG release entry.

## Impact

- Affected specs: new `docs/migration/FROM_KUZU.md`, README callout.
- Affected code: new `scripts/migration/from_kuzu.py`.
- Breaking change: NO.
- User benefit: captures the Kuzu vacancy before forks consolidate; widens user base without engineering churn.
