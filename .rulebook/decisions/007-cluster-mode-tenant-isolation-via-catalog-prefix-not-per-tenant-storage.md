# 7. Cluster-mode tenant isolation via catalog-prefix, not per-tenant storage

**Status**: proposed
**Date**: 2026-04-19
**Related Tasks**: phase5_implement-cluster-mode

## Context

Nexus's cluster mode (phase5_implement-cluster-mode) needs to provide strong multi-tenant isolation — two tenants sharing one server must not see each other's nodes / relationships / properties. The proposal defaulted to "namespace-prefix every storage key" (`storage-format.md` changes), which would have required deep edits to the catalog, page cache, record stores (32 B node / 48 B relationship — no room for a namespace byte), label bitmap index, KNN index, and every operator that touches them. The risk of a data-leak bug on any one of those paths was high, and the implementation window measured in weeks.

## Decision

Adopt a catalog-prefix isolation strategy, modelled after Vectorizer's `hub::TenantIsolationMode::Collection`. Every label / relationship-type / property-key name is registered in the catalog as `ns:<tenant>:<name>` instead of the bare user-visible name. A single AST-walking pass (`cluster::scope::scope_query`) rewrites names in the parsed `CypherQuery` before planning, and the executor consumes the scoped AST via a one-shot `preparsed_ast_override` slot on `ExecutorShared` so it never round-trips through the parser (where `:Label:Label` multi-label syntax would split our prefix). Record stores, page cache, WAL, label bitmap, KNN index, and all operators are untouched — they continue to deal in label IDs, which are tenant-distinct because their *names* are tenant-distinct in the catalog. Isolation propagates for free through every downstream layer.

## Alternatives Considered

- Per-tenant database via the existing DatabaseManager. Stronger isolation (different Engine instances) but higher memory cost per tenant (each Engine carries catalog + WAL + page cache + indexes) and no cross-tenant observability from a single admin endpoint.
- Namespace-prefix every record-store / catalog / index key at the byte level. Most flexible but requires invasive edits to 5+ subsystems and the fixed-size node (32 B) / relationship (48 B) records have no room for a namespace byte, forcing a secondary lookup.
- Row-level ACL stored alongside each node. Defers all enforcement to query-time filtering, which is both slower and more bug-prone — the first missed filter is a leak.

## Consequences

PROS: (1) Isolation covers labels, relationship types, property keys, and — transitively — every node / relationship that references them, with ~600 lines of code in `cluster/` rather than thousands of storage-layer edits. (2) Storage records are untouched, so all existing optimisations (SIMD, columnar cache warming, KNN vector ops) keep working on tenant data with zero change. (3) Standalone mode is byte-identical to pre-cluster-mode behaviour because the walker short-circuits when `isolation == None`. (4) Proven in production by Vectorizer against HiveHub. CONS: (1) Catalog grows O(tenants × distinct-names) rather than O(distinct-names) — not a real concern at expected scale. (2) `MATCH (n)` without a label no longer isolates, because the catalog-prefix trick requires a label to hook onto; mitigation is that cluster-mode deployments are expected to use labelled queries (they already do for indexing). (3) Cross-tenant admin queries (e.g. "total storage for tenant X") need to be implemented separately via the quota provider / usage telemetry, not via raw Cypher — which matches the operational model anyway. (4) One piece of plumbing — the `preparsed_ast_override` slot — exists purely to bypass the executor's re-parse step; it is a small, contained piece of awkwardness but it is visible at the handoff boundary.
