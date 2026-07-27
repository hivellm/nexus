# 10. Null property values are never indexed; MERGE rejects null keys (Neo4j parity)

**Status**: proposed
**Date**: 2026-06-07
**Related Tasks**: phase6_clean-graph-rebuild-null-ids

## Context

Legacy nodes ingested before the 2.3.x fixes carried `null` id/name property values. Null-keyed nodes can't be addressed by an index seek (MATCH (n:L {id: $v}) never matches null), and null values polluted the typed property index and label scans. Needed a defined, Neo4j-aligned contract for null property values plus a deterministic clean-rebuild path.

## Decision

Adopt Neo4j semantics: a property whose value is null is treated as absent. (1) Null values are never indexed — PropertyIndex::add_property no-ops on PropertyValue::Null, so find_exact(..,Null) always returns empty and null-valued properties cannot pollute or be addressed by index seeks. (2) MERGE (n:Label {key: null}) is rejected before match-or-create with a Neo4j-parity error: "Cannot merge node using null property value for {key}" (engine process_merge_clause). The clean rebuild path (DROP DATABASE -> recreate -> recreate indexes -> re-ingest) already works and rebuilds indexes on ingest; documented in docs/ops/graph-rebuild.md. The downstream re-ingest is client-driven (Cortex bootstrap).

## Alternatives Considered

- Index null values and special-case the seek to skip them (rejected: pollutes the tree, diverges from Neo4j, more complex reads)
- Silently create a null-keyed node on MERGE (rejected: diverges from Neo4j, perpetuates the legacy pollution)
- Treat the whole item as docs-only downstream re-bootstrap (rejected: index pollution and MERGE null acceptance are real Nexus-side defects)

## Consequences

Index seeks are deterministic and free of null-keyed pollution. MERGE behavior matches Neo4j exactly. add_property null-skip is the single central enforcement point covering create/merge/batch write paths. No breaking change to non-null behavior. Parameterized MERGE property values remain unsupported in the write path (pre-existing, tracked as issue #7 "parameters: null") — the null guard does not regress this.
