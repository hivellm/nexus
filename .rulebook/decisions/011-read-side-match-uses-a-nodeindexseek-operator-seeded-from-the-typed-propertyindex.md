# 11. Read-side MATCH uses a NodeIndexSeek operator seeded from the typed PropertyIndex

**Status**: proposed
**Date**: 2026-06-07
**Related Tasks**: phase6_fix-read-match-index-seek

## Context

On 2.3.0 the read path (Executor operator pipeline, separate from the engine write path) planned MATCH (n:Label {prop: val}) as NodeByLabel (full label scan) + Filter; try_index_based_filter was a dead stub. Comma-joined MATCH (a),(b) cross-producted two full scans (O(N^2)), timing out edge-upsert endpoint resolution on large graphs (GH #8). The read Executor had no handle to the engine's typed crate::index::PropertyIndex.

## Decision

Thread the engine's typed PropertyIndex (Arc-shared, installed on the executor at construction and refresh) into ExecutorShared, and have the read planner emit a new Operator::NodeIndexSeek { label_id, key_id, value, variable } for an inline equality selector whose (label,key) has a registered index and whose value is an indexable literal (String/Integer/Float/Boolean). NodeIndexSeek seeds the scan from PropertyIndex::find_exact (O(matches)); residual Filter operators still run for full correctness. The PropertyValue built at plan time uses the same normalization as the write-side json_to_property_value so the seek never wrongly excludes a row. Per-leg seeks make comma-joins point lookups, so the existing cartesian step operates on tiny seed sets — no cartesian-logic change needed. The write-path MERGE endpoint resolution already seeks via find_nodes_by_node_pattern's find_exact fast path; both paths now share the find_exact API.

## Alternatives Considered

- Finish the try_index_based_filter stub against the cache string-keyed PropertyIndexManager (rejected: that index is never populated and is unrelated to the typed index that CREATE INDEX writes)
- Rewrite the comma-join into a bitmap-intersection/hash-join (rejected: unnecessary once each leg is a point lookup; far higher risk)
- Route read MATCH through the engine write-path find_nodes_by_node_pattern (rejected: conflates read/write execution models)

## Consequences

Single-node indexed MATCH is O(log N)/O(matches) instead of O(N); comma-join endpoints resolve via independent seeks instead of an O(N^2) cross-product. Results unchanged. Null/Point/parameter/non-literal selectors fall back to NodeByLabel (null never indexed per the null-key contract). After a restart without re-declaring indexes, has_index is false and reads fall back to label scan (same as before). Plan-structure tests assert NodeIndexSeek emission (indexed) vs NodeByLabel (unindexed) and 2 seeks for an indexed comma-join.
