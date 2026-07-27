//! `QueryPlanner` construction, builder shims, the query-hash helper,
//! and `plan_query` — the top-level planning entry point that routes
//! to either [`super::bound::plan_query_bound`] (single-segment plans)
//! or [`super::segments::plan_segmented`] (queries with a `MATCH`
//! after a `WITH`).

use super::super::*;

impl<'a> QueryPlanner<'a> {
    /// Create a new query planner without an R-tree registry handle.
    ///
    /// Plans built by this constructor never emit `Operator::SpatialSeek`
    /// — every spatial predicate falls back to `NodeByLabel + Filter`.
    /// Use [`QueryPlanner::with_rtree`] to opt into the rewriter from
    /// callers that hold a registry handle (`Engine::execute_*` paths).
    pub fn new(catalog: &'a Catalog, label_index: &'a LabelIndex, knn_index: &'a KnnIndex) -> Self {
        Self {
            catalog,
            label_index,
            knn_index,
            rtree_registry: None,
            property_index: None,
            composite_index: None,
            plan_cache: QueryPlanCache::new(1000, Duration::from_secs(300)), // 1000 plans, 5min TTL
            aggregation_cache: AggregationCache::new(500, Duration::from_secs(180)), // 500 results, 3min TTL
            notifications: Vec::new(),
        }
    }

    /// Drain the notifications accumulated during the most recent
    /// `plan_query` call. Call site (`Engine::execute_*`) attaches the
    /// drained vector to the resulting `ResultSet` so the HTTP layer
    /// can copy it into the `/cypher` response envelope. The internal
    /// vector is replaced with an empty one — reusing the same
    /// planner for a follow-up query is safe.
    pub fn take_notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.notifications)
    }

    /// Builder shim: install an R-tree registry handle so the
    /// spatial-seek rewriter (phase6_spatial-planner-seek §2) can
    /// look up which `(label, property)` pairs have a registered
    /// index. Idiomatic call: `QueryPlanner::new(...).with_rtree(reg)`.
    pub fn with_rtree(
        mut self,
        registry: std::sync::Arc<crate::index::rtree::RTreeRegistry>,
    ) -> Self {
        self.rtree_registry = Some(registry);
        self
    }

    /// Builder shim: install a property-index handle so
    /// `USING INDEX <var>:<Label>(<prop>)` hints can be validated at
    /// plan time (phase7_planner-using-index-hints). When the named
    /// `(label, property)` pair has no registered index the planner
    /// raises `ERR_USING_INDEX_NOT_FOUND`. Without a handle the hint
    /// is accepted silently, matching the legacy behaviour of
    /// callers that have no `IndexManager` reference (planner unit
    /// tests, the standalone `Executor::parse_and_plan`).
    pub fn with_property_index(mut self, idx: &'a crate::index::PropertyIndex) -> Self {
        self.property_index = Some(idx);
        self
    }

    /// Builder shim: install a composite B-tree index registry handle
    /// so an inline multi-property selector (`MATCH (n:L {a: 1, b:
    /// 2})`) can seek a registered composite index / NODE KEY
    /// constraint instead of falling back to a single-property index
    /// seek or a full label scan. Idiomatic call:
    /// `QueryPlanner::new(...).with_composite_index(reg)`. Without a
    /// handle the planner never emits `Operator::CompositeBtreeSeek`,
    /// matching the legacy behaviour of callers with no index handle.
    pub fn with_composite_index(
        mut self,
        registry: &'a crate::index::composite_btree::CompositeBtreeRegistry,
    ) -> Self {
        self.composite_index = Some(registry);
        self
    }

    /// Generate a hash for query caching based on query structure
    pub(in super::super) fn hash_query(&self, query: &CypherQuery) -> u64 {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();

        // Hash the clauses (this captures the query structure)
        for clause in &query.clauses {
            clause.hash(&mut hasher);
        }

        // Hash parameters if they affect planning (for now, ignore runtime parameters)
        // In a full implementation, we'd hash parameter types but not values

        hasher.finish()
    }

    /// Plan a Cypher query into optimized operators with caching
    pub fn plan_query(&mut self, query: &CypherQuery) -> Result<Vec<Operator>> {
        // A `MATCH` that follows a `WITH` opens a new query segment whose
        // pattern variables the "bucket" planner below would otherwise
        // collapse into the pre-`WITH` match phase (phase7 §4.11). Route
        // those queries through the segment planner, which plans each
        // `WITH`-delimited segment in order and threads the carried bindings
        // across the boundary. Every other query keeps the single-segment
        // path unchanged.
        if Self::has_match_after_with(query) {
            return self.plan_segmented(query);
        }
        self.plan_query_bound(query, &std::collections::HashSet::new())
    }
}
