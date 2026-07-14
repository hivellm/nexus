//! `impl QueryPlanner` constructor, builder shims, and `plan_query` —
//! the top-level planning entry point.

use super::*;

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

    /// Generate a hash for query caching based on query structure
    pub(super) fn hash_query(&self, query: &CypherQuery) -> u64 {
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
        // Reset planner-level notifications: every plan starts with a
        // clean slate so the engine drains only the notes produced by
        // *this* query. Notifications from a prior call belong to the
        // prior `ResultSet`.
        self.notifications.clear();

        // Diagnostic pre-pass: scan MATCH/MERGE selectors for property
        // predicates against unindexed `(label, property)` pairs. Runs
        // before plan-cache lookup so a cache hit still surfaces the
        // hint — operators only see the index pathology after the
        // first plan, but the warning matters every time the query
        // runs against the wedged catalog.
        self.scan_unindexed_property_access(query);

        // Validate that query has at least one clause
        // Exceptions: CALL procedures and USE DATABASE can be standalone
        if query.clauses.is_empty() {
            return Err(Error::CypherSyntax(
                "Query must contain at least one clause".to_string(),
            ));
        }

        // Check if query is just a CALL procedure or USE DATABASE (can be standalone)
        if query.clauses.len() == 1 {
            match &query.clauses[0] {
                Clause::CallProcedure(_) => {
                    // CALL procedures can be standalone - they produce their own columns/rows
                    // No need for RETURN clause
                }
                Clause::UseDatabase(_) => {
                    // USE DATABASE should be handled at server level, not by planner
                    // But if it reaches here, we'll allow it to pass through
                }
                _ => {}
            }
        }

        // Check if query contains UNION - if so, split and plan separately
        // NOTE: This check must come BEFORE cache lookup, as UNION queries need to be split
        // and each side planned separately (they may have different labels/variables)
        // Also, UNION sub-queries should not use cache to avoid conflicts
        let has_union = query.clauses.iter().any(|c| matches!(c, Clause::Union(_)));

        let query_hash = self.hash_query(query);

        if has_union {
            if let Some(union_idx) = query
                .clauses
                .iter()
                .position(|c| matches!(c, Clause::Union(_)))
            {
                // Extract the UnionClause to get union_type
                let distinct =
                    if let Some(Clause::Union(union_clause)) = query.clauses.get(union_idx) {
                        union_clause.union_type == crate::executor::parser::UnionType::Distinct
                    } else {
                        true // Default to UNION (distinct)
                    };

                // Find where ORDER BY and LIMIT clauses start (after UNION)
                // These should be applied to the combined UNION results, not individual branches
                let mut right_end_idx = query.clauses.len();
                for i in union_idx + 1..query.clauses.len() {
                    match &query.clauses[i] {
                        Clause::OrderBy(_) | Clause::Limit(_) => {
                            // ORDER BY and LIMIT are processed after UNION, not as part of right side
                            right_end_idx = i;
                            break;
                        }
                        Clause::Union(_) => {
                            // Another UNION found - this is part of the right side
                            // We'll handle multiple UNIONs in the right side recursively
                            break;
                        }
                        _ => {
                            // Other clauses are part of the right side
                        }
                    }
                }

                // Split query into left and right parts (excluding ORDER BY/LIMIT after UNION)
                let left_clauses: Vec<Clause> = query.clauses[..union_idx].to_vec();
                let right_clauses: Vec<Clause> =
                    query.clauses[union_idx + 1..right_end_idx].to_vec();

                // Extract ORDER BY and LIMIT clauses that come after UNION
                let mut post_union_order_by: Option<(Vec<String>, Vec<bool>)> = None;
                let mut post_union_limit: Option<usize> = None;

                for clause in query.clauses.iter().skip(right_end_idx) {
                    match clause {
                        Clause::OrderBy(order_by_clause) => {
                            // Collect ORDER BY clause to add after UNION
                            let mut columns = Vec::new();
                            let mut ascending = Vec::new();

                            for item in &order_by_clause.items {
                                // Convert expression to column name
                                let column = self.expression_to_string(&item.expression)?;
                                columns.push(column);

                                // Convert direction
                                let is_asc = item.direction == SortDirection::Ascending;
                                ascending.push(is_asc);
                            }

                            post_union_order_by = Some((columns, ascending));
                        }
                        Clause::Limit(limit_clause) => {
                            if let Expression::Literal(Literal::Integer(count)) =
                                &limit_clause.count
                            {
                                post_union_limit = Some(*count as usize);
                            }
                        }
                        _ => {
                            // Other clauses after UNION are not supported (e.g., SKIP, another UNION)
                            // For now, we'll skip them
                        }
                    }
                }

                // Create separate queries for left and right (without ORDER BY/LIMIT)
                let left_query = CypherQuery {
                    clauses: left_clauses,
                    params: query.params.clone(),
                    graph_scope: query.graph_scope.clone(),
                };
                let right_query = CypherQuery {
                    clauses: right_clauses,
                    params: query.params.clone(),
                    graph_scope: query.graph_scope.clone(),
                };

                // Plan both sides recursively
                // For UNION sub-queries, we need to ensure each side is planned independently
                // Temporarily disable cache to prevent sub-queries from reusing cached plans
                // This is important because the left and right sides may have different labels/variables
                // but could hash to the same value if only the query structure is considered
                let mut temp_planner = QueryPlanner {
                    catalog: self.catalog,
                    label_index: self.label_index,
                    knn_index: self.knn_index,
                    rtree_registry: self.rtree_registry.clone(),
                    property_index: self.property_index,
                    plan_cache: QueryPlanCache::new(0, std::time::Duration::from_secs(0)), // Empty cache
                    aggregation_cache: AggregationCache::new(
                        100,
                        std::time::Duration::from_secs(3600),
                    ),
                    notifications: Vec::new(),
                };
                let left_operators = temp_planner.plan_query(&left_query)?;
                let right_operators = temp_planner.plan_query(&right_query)?;
                // Lift sub-planner notifications back into self so the
                // UNION as a whole reports the union of its branches'
                // diagnostics — otherwise hints from inside a UNION
                // would silently drop on the floor.
                self.notifications.extend(temp_planner.take_notifications());

                // Create UNION operator with complete operator pipelines for each side
                let mut operators = vec![Operator::Union {
                    left: left_operators,
                    right: right_operators,
                    distinct,
                }];

                // Add ORDER BY after UNION if present
                if let Some((columns, ascending)) = post_union_order_by {
                    operators.push(Operator::Sort { columns, ascending });
                }

                // Add LIMIT after UNION (and ORDER BY if present) if present
                if let Some(count) = post_union_limit {
                    operators.push(Operator::Limit { count });
                }

                // Cache the UNION plan
                let estimated_cost = 100.0; // Placeholder cost
                self.plan_cache
                    .put(query_hash, operators.clone(), estimated_cost);

                return Ok(operators);
            }
        }

        // Try to get cached plan first (for non-UNION queries)
        if let Some(cached_plan) = self.plan_cache.get(query_hash) {
            // Return cached operators (clone them since they're cached)
            return Ok(cached_plan.operators.clone());
        }

        let mut operators = Vec::new();

        // Extract patterns and constraints
        let mut patterns = Vec::new();
        // WHERE clauses with optional variable context: (expression, optional_vars)
        // optional_vars is non-empty if this WHERE follows an OPTIONAL MATCH
        let mut where_clauses: Vec<(Expression, Vec<String>)> = Vec::new();
        // Track variables from the most recent OPTIONAL MATCH
        let mut last_optional_vars: Vec<String> = Vec::new();
        let mut return_items = Vec::new();
        let mut limit_count = None;
        let mut return_distinct = false;
        let mut unwind_operators = Vec::new(); // Collect UNWIND to insert after MATCH
        let mut create_patterns: Vec<(
            crate::executor::parser::Pattern,
            Option<crate::executor::parser::Expression>,
            crate::executor::parser::AstConflictPolicy,
        )> = Vec::new(); // Collect CREATE to insert after MATCH
        let mut with_operators: Vec<(Vec<ReturnItem>, bool, Option<Expression>)> = Vec::new(); // Collect WITH clauses with optional WHERE
        let mut with_has_aggregation = false; // Track if WITH clause has aggregation
        let mut with_aggregation_where: Option<Expression> = None; // Track WHERE from WITH with aggregation
        // phase6 §5 — When WITH carries the aggregation and RETURN only
        // references the WITH aliases (optionally wrapping them in a
        // non-aggregate expression like `hi > 0.99`), keep RETURN's
        // items separately so we can emit a post-aggregation Project
        // that evaluates them. Pre-fix the RETURN items were silently
        // discarded and the WITH projection leaked through.
        let mut post_aggregation_return_items: Option<Vec<ReturnItem>> = None;
        let mut match_hints = Vec::new(); // Collect hints from MATCH clauses
        let mut order_by_clause: Option<(Vec<String>, Vec<bool>)> = None; // Collect ORDER BY to add after projection

        // Track if UNWIND appears before MATCH in the query
        // This is needed for queries like: UNWIND [...] AS x MATCH (p:Person {name: x})
        // where UNWIND must run before MATCH to bind the variable
        let mut unwind_before_match = false;
        {
            let mut seen_match = false;
            for clause in &query.clauses {
                match clause {
                    Clause::Unwind(_) => {
                        if !seen_match {
                            unwind_before_match = true;
                        }
                    }
                    Clause::Match(_) => {
                        seen_match = true;
                    }
                    _ => {}
                }
            }
        }

        for clause in &query.clauses {
            match clause {
                Clause::Match(match_clause) => {
                    // Store pattern with optional flag for LEFT OUTER JOIN semantics
                    patterns.push((match_clause.pattern.clone(), match_clause.optional));

                    // Extract variables from this MATCH for optional context
                    if match_clause.optional {
                        // Extract target and relationship variables from OPTIONAL MATCH pattern
                        // IMPORTANT: Skip the first node as it's typically the "anchor" that's already bound
                        // Only include variables from subsequent nodes and relationships
                        last_optional_vars.clear();
                        let mut is_first_node = true;
                        for element in &match_clause.pattern.elements {
                            match element {
                                PatternElement::Node(node) => {
                                    if is_first_node {
                                        // Skip the first node - it's the anchor from previous MATCH
                                        is_first_node = false;
                                    } else if let Some(var) = &node.variable {
                                        last_optional_vars.push(var.clone());
                                    }
                                }
                                PatternElement::Relationship(rel) => {
                                    if let Some(var) = &rel.variable {
                                        last_optional_vars.push(var.clone());
                                    }
                                }
                                PatternElement::QuantifiedGroup(group) => {
                                    for inner in &group.inner {
                                        match inner {
                                            PatternElement::Node(n) => {
                                                if let Some(var) = &n.variable {
                                                    last_optional_vars.push(var.clone());
                                                }
                                            }
                                            PatternElement::Relationship(r) => {
                                                if let Some(var) = &r.variable {
                                                    last_optional_vars.push(var.clone());
                                                }
                                            }
                                            PatternElement::QuantifiedGroup(_) => {}
                                        }
                                    }
                                }
                            }
                        }
                        tracing::debug!(
                            "PLANNER: OPTIONAL MATCH detected, tracking NEW vars (excluding anchor): {:?}",
                            last_optional_vars
                        );
                    } else {
                        // Regular MATCH clears optional context
                        last_optional_vars.clear();
                    }

                    if let Some(where_clause) = &match_clause.where_clause {
                        // WHERE inside MATCH clause inherits the optional context from this MATCH
                        let opt_vars = if match_clause.optional {
                            last_optional_vars.clone()
                        } else {
                            Vec::new()
                        };
                        where_clauses.push((where_clause.expression.clone(), opt_vars));
                    }
                    // Collect hints from first MATCH clause
                    if match_hints.is_empty() {
                        match_hints = match_clause.hints.clone();
                    }
                    // OPTIONAL MATCH is handled by executor as LEFT OUTER JOIN semantics
                    // The optional flag is passed to plan_execution_strategy for proper handling
                    // Query hints are stored in match_clause.hints and will be used during planning
                }
                Clause::Create(create_clause) => {
                    // Collect CREATE patterns to add AFTER MATCH operators
                    create_patterns.push((
                        create_clause.pattern.clone(),
                        create_clause.external_id_expr.clone(),
                        create_clause.conflict_policy,
                    ));
                }
                Clause::Delete(delete_clause) => {
                    // Extract variables to delete from the delete clause
                    let variables = delete_clause.items.clone();

                    if delete_clause.detach {
                        operators.push(Operator::DetachDelete { variables });
                    } else {
                        operators.push(Operator::Delete { variables });
                    }
                }
                Clause::Merge(merge_clause) => {
                    patterns.push((merge_clause.pattern.clone(), false)); // MERGE is never optional
                    // MERGE is handled as match-or-create
                    // Store pattern for executor to handle
                }
                Clause::Where(where_clause) => {
                    // Standalone WHERE inherits optional context from last OPTIONAL MATCH
                    where_clauses
                        .push((where_clause.expression.clone(), last_optional_vars.clone()));
                }
                Clause::With(with_clause) => {
                    // WITH clause - collect for later processing
                    // WITH creates intermediate scope with aliased variables

                    // Store the WHERE clause expression (if present) to be applied AFTER the WITH projection
                    let where_expr = with_clause
                        .where_clause
                        .as_ref()
                        .map(|wc| wc.expression.clone());
                    with_operators.push((
                        with_clause.items.clone(),
                        with_clause.distinct,
                        where_expr,
                    ));

                    // Check if WITH clause has aggregation
                    for item in &with_clause.items {
                        if self.contains_aggregation(&item.expression) {
                            with_has_aggregation = true;
                            // Store the WHERE clause to be applied after aggregation
                            if let Some(wc) = &with_clause.where_clause {
                                with_aggregation_where = Some(wc.expression.clone());
                            }
                            break;
                        }
                    }

                    // NOTE: We do NOT add WITH's WHERE clause to global where_clauses
                    // because it must be applied AFTER the WITH projection, not before

                    // Set return_items - these will be used for aggregation planning
                    // If RETURN comes later WITHOUT aggregation and WITH has aggregation,
                    // we keep WITH's items for aggregation processing
                    return_items = with_clause.items.clone();
                    return_distinct = with_clause.distinct;
                }
                Clause::Unwind(unwind_clause) => {
                    // UNWIND expands a list into rows
                    // Collect to insert after MATCH operators
                    let expression_str = self.expression_to_string(&unwind_clause.expression)?;
                    unwind_operators.push(Operator::Unwind {
                        expression: expression_str,
                        variable: unwind_clause.variable.clone(),
                    });
                }
                Clause::Return(return_clause) => {
                    // If WITH had aggregation, check if RETURN is just referencing the aliases
                    // In that case, keep WITH's items for aggregation planning
                    let return_has_agg = return_clause
                        .items
                        .iter()
                        .any(|item| self.contains_aggregation(&item.expression));

                    if with_has_aggregation && !return_has_agg {
                        // WITH had aggregation, RETURN is just referencing aliases
                        // Don't overwrite - let aggregation planning use WITH items
                        // But we need to update the aliases to match RETURN
                        tracing::debug!(
                            "WITH has aggregation, RETURN references aliases - keeping WITH items"
                        );
                        // phase6 §5 — Stash RETURN's items so the planner
                        // can emit a Project AFTER the Aggregate that
                        // evaluates expressions like `hi > 0.99 AS any_high`
                        // on top of the aggregated aliases. Previously
                        // these items were silently dropped and the
                        // aggregation's raw output shape leaked through
                        // as the final result.
                        post_aggregation_return_items = Some(return_clause.items.clone());
                        return_distinct = return_clause.distinct || return_distinct;
                    } else {
                        return_items = return_clause.items.clone();
                        return_distinct = return_clause.distinct;
                    }
                }
                Clause::Limit(limit_clause) => {
                    if let Expression::Literal(Literal::Integer(count)) = &limit_clause.count {
                        limit_count = Some(*count as usize);
                    }
                }
                Clause::OrderBy(order_by_clause_parsed) => {
                    // Collect ORDER BY clause to add after projection
                    // We'll resolve these to column aliases later
                    let mut columns = Vec::new();
                    let mut ascending = Vec::new();

                    for item in &order_by_clause_parsed.items {
                        // Convert expression to column name
                        // This will be resolved to alias after we know the RETURN items
                        let column = self.expression_to_string(&item.expression)?;
                        columns.push(column);

                        // Convert direction
                        let is_asc = item.direction == SortDirection::Ascending;
                        ascending.push(is_asc);
                    }

                    // Store for later addition and resolution
                    order_by_clause = Some((columns, ascending));
                }
                Clause::Union(_) => {
                    // Should have been handled above
                }
                Clause::CallProcedure(call_procedure_clause) => {
                    // Add CallProcedure operator
                    operators.push(Operator::CallProcedure {
                        procedure_name: call_procedure_clause.procedure_name.clone(),
                        arguments: call_procedure_clause.arguments.clone(),
                        yield_columns: call_procedure_clause.yield_columns.clone(),
                    });
                }
                Clause::LoadCsv(load_csv_clause) => {
                    // Add LoadCsv operator
                    operators.push(Operator::LoadCsv {
                        url: load_csv_clause.url.clone(),
                        variable: load_csv_clause.variable.clone(),
                        with_headers: load_csv_clause.with_headers,
                        field_terminator: load_csv_clause.field_terminator.clone(),
                    });
                }
                Clause::CreateIndex(create_index_clause) => {
                    // Add CreateIndex operator
                    operators.push(Operator::CreateIndex {
                        label: create_index_clause.label.clone(),
                        property: create_index_clause.property.clone(),
                        index_type: create_index_clause.index_type.clone(),
                        if_not_exists: create_index_clause.if_not_exists,
                        or_replace: create_index_clause.or_replace,
                    });
                }
                Clause::ShowDatabases => {
                    // Add ShowDatabases operator
                    operators.push(Operator::ShowDatabases);
                }
                Clause::CreateDatabase(create_db_clause) => {
                    // Add CreateDatabase operator
                    operators.push(Operator::CreateDatabase {
                        name: create_db_clause.name.clone(),
                        if_not_exists: create_db_clause.if_not_exists,
                    });
                }
                Clause::DropDatabase(drop_db_clause) => {
                    // Add DropDatabase operator
                    operators.push(Operator::DropDatabase {
                        name: drop_db_clause.name.clone(),
                        if_exists: drop_db_clause.if_exists,
                    });
                }
                Clause::AlterDatabase(alter_db_clause) => {
                    // Add AlterDatabase operator
                    use crate::executor::parser::DatabaseAlteration;
                    let (read_only, option) = match &alter_db_clause.alteration {
                        DatabaseAlteration::SetAccess { read_only } => (Some(*read_only), None),
                        DatabaseAlteration::SetOption { key, value } => {
                            (None, Some((key.clone(), value.clone())))
                        }
                    };
                    operators.push(Operator::AlterDatabase {
                        name: alter_db_clause.name.clone(),
                        read_only,
                        option,
                    });
                }
                Clause::UseDatabase(use_db_clause) => {
                    // Add UseDatabase operator
                    operators.push(Operator::UseDatabase {
                        name: use_db_clause.name.clone(),
                    });
                }
                Clause::CallSubquery(call_sub) => {
                    // phase6_opencypher-subquery-transactions §4 / §8
                    // — emit a `CallSubquery` operator that runs the
                    // inner AST once per outer driver row, with
                    // optional batched/transactional semantics and
                    // an optional import-list narrowing the inner
                    // scope to the listed outer variables only.
                    operators.push(Operator::CallSubquery {
                        inner_query: call_sub.query.clone(),
                        in_transactions: call_sub.in_transactions,
                        batch_size: call_sub.batch_size,
                        concurrency: call_sub.concurrency,
                        on_error: call_sub.on_error.clone(),
                        status_var: call_sub.status_var.clone(),
                        import_list: call_sub.import_list.clone(),
                    });
                }
                _ => {
                    // Other clauses not implemented in MVP
                }
            }
        }

        // Plan execution strategy only if we have patterns to match
        // CREATE-only queries don't need pattern matching
        if !patterns.is_empty() {
            self.plan_execution_strategy(
                &patterns,
                &where_clauses,
                &return_items,
                limit_count,
                return_distinct,
                &unwind_operators,
                unwind_before_match,
                &match_hints,
                &order_by_clause,
                &with_aggregation_where,
                &mut operators,
            )?;
        }

        // phase6 §5 — append a Project for RETURN's items when WITH
        // carried the aggregation. The aggregation planner built its
        // operator tree from WITH's items (so `count(n) AS total` and
        // `max(n.score) AS hi` are emitted correctly by the Aggregate
        // operator), but the RETURN's expressions (e.g. `hi > 0.99 AS
        // any_high`) still need to run on top of the aggregate's
        // output. Without this step the bench's subquery.exists_high_score,
        // subquery.size_of_collect, and subquery.with_filter_count
        // scenarios returned the raw aggregation shape instead of the
        // projected RETURN shape.
        if let Some(ref final_items) = post_aggregation_return_items {
            let projection_items: Vec<ProjectionItem> = final_items
                .iter()
                .map(|item| {
                    let alias = item.alias.clone().unwrap_or_else(|| {
                        self.expression_to_string(&item.expression)
                            .unwrap_or_else(|_| "expr".to_string())
                    });
                    ProjectionItem {
                        alias,
                        expression: item.expression.clone(),
                    }
                })
                .collect();

            // Insert before LIMIT if it exists, otherwise append.
            // Also keep it AFTER any ORDER BY Sort so sorting happens on
            // pre-projection values (openCypher allows ORDER BY
            // referencing WITH aliases that the RETURN projection might
            // rename away).
            let insert_pos = operators
                .iter()
                .position(|op| matches!(op, Operator::Limit { .. }))
                .unwrap_or(operators.len());
            operators.insert(
                insert_pos,
                Operator::Project {
                    items: projection_items,
                },
            );
        }

        // Add UNWIND operators BEFORE WITH when there are no patterns AND there are WITH operators
        // This ensures UNWIND generates rows before WITH transforms them
        // Only add here if WITH operators exist (otherwise UNWIND is added later in the no-patterns block)
        if patterns.is_empty() && !unwind_operators.is_empty() && !with_operators.is_empty() {
            operators.extend(unwind_operators.clone());
        }

        // Add WITH operators AFTER MATCH/Filter/UNWIND but BEFORE Project
        // This ensures WITH intermediate projections run and create aliased variables
        // Skip WITH operators that contain aggregation - they are handled by Aggregate operator
        for (with_items, with_distinct, where_expr) in with_operators.iter() {
            // Check if WITH has aggregation - if so, skip (Aggregate operator handles it)
            // Note: with_aggregation_where is already set earlier in the WITH clause processing
            let has_agg = with_items
                .iter()
                .any(|item| self.contains_aggregation(&item.expression));
            if has_agg {
                tracing::debug!(
                    "Skipping WITH operator generation - has aggregation (handled by Aggregate)"
                );
                continue;
            }

            // Convert ReturnItems to ProjectionItems
            let projection_items: Vec<ProjectionItem> = with_items
                .iter()
                .map(|item| {
                    let alias = item.alias.clone().unwrap_or_else(|| {
                        self.expression_to_string(&item.expression)
                            .unwrap_or_else(|_| "expr".to_string())
                    });
                    ProjectionItem {
                        alias,
                        expression: item.expression.clone(),
                    }
                })
                .collect();

            // Find the position to insert WITH:
            // - Before Project OR Aggregate if either exists — WITH's projection
            //   (and its WHERE filter) must run BEFORE aggregation, otherwise
            //   `MATCH (n:A) WITH n.score AS s WHERE s > 0.1 RETURN count(*)`
            //   has Aggregate already collapsing all `n`-bearing rows into
            //   one by the time WITH tries to read `n.score` (phase6 §5.3).
            // - After UNWIND if no sink exists (WITH needs UNWIND data)
            // - At end if neither exists
            let sink_pos = operators
                .iter()
                .position(|op| matches!(op, Operator::Project { .. } | Operator::Aggregate { .. }));

            let insert_pos = if let Some(pos) = sink_pos {
                pos
            } else {
                // No Project/Aggregate found - insert after UNWIND operators (if any)
                // Find the last UNWIND operator position
                let last_unwind_pos = operators
                    .iter()
                    .rposition(|op| matches!(op, Operator::Unwind { .. }));
                last_unwind_pos.map(|p| p + 1).unwrap_or(operators.len())
            };
            operators.insert(
                insert_pos,
                Operator::With {
                    items: projection_items,
                    distinct: *with_distinct,
                },
            );

            // If WITH has a WHERE clause, insert a Filter operator AFTER the WITH operator
            // This ensures the WHERE clause filters the projected WITH variables, not the original variables
            if let Some(where_expression) = where_expr {
                let filter_str = self.expression_to_string(where_expression)?;
                tracing::debug!(
                    "WITH WHERE: Inserting Filter at position {} (after WITH at {})",
                    insert_pos + 1,
                    insert_pos
                );
                operators.insert(
                    insert_pos + 1, // Insert right after the WITH operator we just inserted
                    Operator::Filter {
                        predicate: filter_str,
                    },
                );
                // DEBUG: Show operator order after insertion
                for (idx, op) in operators.iter().enumerate() {
                    let op_name = match op {
                        Operator::NodeByLabel { variable, .. } => {
                            format!("NodeByLabel({})", variable)
                        }
                        Operator::Filter { predicate } => {
                            format!("Filter({})", predicate.chars().take(30).collect::<String>())
                        }
                        Operator::With { items, .. } => format!("With({} items)", items.len()),
                        Operator::Project { items } => format!("Project({} items)", items.len()),
                        _ => format!("{:?}", std::mem::discriminant(op)),
                    };
                    tracing::debug!("  Operator #{}: {}", idx, op_name);
                }
            }
        }

        // Add CREATE operators AFTER MATCH/Filter but BEFORE Project
        // This ensures CREATE runs after all nodes are matched but before
        // the RETURN projection destroys the node objects with _nexus_id
        if !create_patterns.is_empty() {
            // Find the position of the first Project operator
            let project_pos = operators
                .iter()
                .position(|op| matches!(op, Operator::Project { .. }));

            // Insert CREATE operators before Project (or at end if no Project)
            let insert_pos = project_pos.unwrap_or(operators.len());
            for (i, (create_pattern, external_id_expr, conflict_policy)) in
                create_patterns.into_iter().enumerate()
            {
                operators.insert(
                    insert_pos + i,
                    Operator::Create {
                        pattern: create_pattern,
                        external_id_expr,
                        conflict_policy,
                    },
                );
            }
        }

        if patterns.is_empty() && (!return_items.is_empty() || !unwind_operators.is_empty()) {
            // No patterns but have RETURN or UNWIND - check for aggregations first
            // This handles cases like: RETURN count(*), RETURN sum(1), etc.
            // Only add UNWIND here if not already added (when WITH operators exist, UNWIND was added earlier)
            if with_operators.is_empty() {
                operators.extend(unwind_operators);
            }

            // Add filter operators for WHERE clauses (when there are no patterns)
            // This handles cases like: RETURN 42 WHERE false, RETURN 5 WHERE 5 > 10, etc.
            for (where_clause, optional_vars) in &where_clauses {
                let predicate = self.expression_to_string(where_clause)?;
                if optional_vars.is_empty() {
                    operators.push(Operator::Filter { predicate });
                } else {
                    operators.push(Operator::OptionalFilter {
                        predicate,
                        optional_vars: optional_vars.clone(),
                    });
                }
            }

            if !return_items.is_empty() {
                // Check if any return items contain aggregate functions
                let mut has_aggregation = false;
                let mut aggregations = Vec::new();
                let group_by_columns = Vec::new();
                let mut projection_items: Vec<ProjectionItem> = Vec::new();

                for item in &return_items {
                    match &item.expression {
                        Expression::FunctionCall { name, args } => {
                            let func_name = name.to_lowercase();
                            match func_name.as_str() {
                                "count" => {
                                    has_aggregation = true;
                                    let mut distinct = false;
                                    let mut real_args = args.clone();
                                    if let Some(Expression::Variable(var)) = args.first() {
                                        if var == "__DISTINCT__" {
                                            distinct = true;
                                            real_args = args[1..].to_vec();
                                        }
                                    }
                                    let column = if real_args.is_empty() {
                                        None // COUNT(*)
                                    } else if let Some(Expression::Variable(var)) =
                                        real_args.first()
                                    {
                                        Some(var.clone())
                                    } else if let Some(Expression::PropertyAccess {
                                        variable,
                                        property,
                                    }) = real_args.first()
                                    {
                                        Some(format!("{}.{}", variable, property))
                                    } else {
                                        None
                                    };
                                    aggregations.push(Aggregation::Count {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "count".to_string()),
                                        distinct,
                                    });
                                }
                                "sum" => {
                                    has_aggregation = true;
                                    if let Some(arg) = args.first() {
                                        let column = match arg {
                                            Expression::Variable(var) => var.clone(),
                                            Expression::PropertyAccess { variable, property } => {
                                                format!("{}.{}", variable, property)
                                            }
                                            Expression::Literal(_) => {
                                                let alias =
                                                    format!("__sum_arg_{}", aggregations.len());
                                                projection_items.push(ProjectionItem {
                                                    alias: alias.clone(),
                                                    expression: arg.clone(),
                                                });
                                                alias
                                            }
                                            _ => continue,
                                        };
                                        aggregations.push(Aggregation::Sum {
                                            column,
                                            alias: item
                                                .alias
                                                .clone()
                                                .unwrap_or_else(|| "sum".to_string()),
                                        });
                                    }
                                }
                                "avg" => {
                                    has_aggregation = true;
                                    if let Some(arg) = args.first() {
                                        let column = match arg {
                                            Expression::Variable(var) => var.clone(),
                                            Expression::PropertyAccess { variable, property } => {
                                                format!("{}.{}", variable, property)
                                            }
                                            Expression::Literal(_) => {
                                                let alias =
                                                    format!("__avg_arg_{}", aggregations.len());
                                                projection_items.push(ProjectionItem {
                                                    alias: alias.clone(),
                                                    expression: arg.clone(),
                                                });
                                                alias
                                            }
                                            _ => continue,
                                        };
                                        aggregations.push(Aggregation::Avg {
                                            column,
                                            alias: item
                                                .alias
                                                .clone()
                                                .unwrap_or_else(|| "avg".to_string()),
                                        });
                                    }
                                }
                                "min" => {
                                    has_aggregation = true;
                                    if let Some(arg) = args.first() {
                                        let column = match arg {
                                            Expression::Variable(var) => var.clone(),
                                            Expression::PropertyAccess { variable, property } => {
                                                format!("{}.{}", variable, property)
                                            }
                                            Expression::Literal(_) => {
                                                let alias =
                                                    format!("__min_arg_{}", aggregations.len());
                                                projection_items.push(ProjectionItem {
                                                    alias: alias.clone(),
                                                    expression: arg.clone(),
                                                });
                                                alias
                                            }
                                            _ => continue,
                                        };
                                        aggregations.push(Aggregation::Min {
                                            column,
                                            alias: item
                                                .alias
                                                .clone()
                                                .unwrap_or_else(|| "min".to_string()),
                                        });
                                    }
                                }
                                "max" => {
                                    has_aggregation = true;
                                    if let Some(arg) = args.first() {
                                        let column = match arg {
                                            Expression::Variable(var) => var.clone(),
                                            Expression::PropertyAccess { variable, property } => {
                                                format!("{}.{}", variable, property)
                                            }
                                            Expression::Literal(_) => {
                                                let alias =
                                                    format!("__max_arg_{}", aggregations.len());
                                                projection_items.push(ProjectionItem {
                                                    alias: alias.clone(),
                                                    expression: arg.clone(),
                                                });
                                                alias
                                            }
                                            _ => continue,
                                        };
                                        aggregations.push(Aggregation::Max {
                                            column,
                                            alias: item
                                                .alias
                                                .clone()
                                                .unwrap_or_else(|| "max".to_string()),
                                        });
                                    }
                                }
                                // phase6 §9 — statistical aggregations: same shape as
                                // min / max / avg. Without these arms the planner fell
                                // through to the "not an aggregate" branch and emitted
                                // a per-row projection — so `MATCH (n:A) RETURN
                                // stdev(n.score)` returned one row per node instead of
                                // one aggregated row.
                                "stdev" => {
                                    has_aggregation = true;
                                    if let Some(arg) = args.first() {
                                        let column = match arg {
                                            Expression::Variable(var) => var.clone(),
                                            Expression::PropertyAccess { variable, property } => {
                                                format!("{}.{}", variable, property)
                                            }
                                            _ => continue,
                                        };
                                        aggregations.push(Aggregation::StDev {
                                            column,
                                            alias: item
                                                .alias
                                                .clone()
                                                .unwrap_or_else(|| "stdev".to_string()),
                                        });
                                    }
                                }
                                "stdevp" => {
                                    has_aggregation = true;
                                    if let Some(arg) = args.first() {
                                        let column = match arg {
                                            Expression::Variable(var) => var.clone(),
                                            Expression::PropertyAccess { variable, property } => {
                                                format!("{}.{}", variable, property)
                                            }
                                            _ => continue,
                                        };
                                        aggregations.push(Aggregation::StDevP {
                                            column,
                                            alias: item
                                                .alias
                                                .clone()
                                                .unwrap_or_else(|| "stdevp".to_string()),
                                        });
                                    }
                                }
                                "percentilecont" => {
                                    has_aggregation = true;
                                    if args.len() >= 2 {
                                        let column = match &args[0] {
                                            Expression::Variable(var) => var.clone(),
                                            Expression::PropertyAccess { variable, property } => {
                                                format!("{}.{}", variable, property)
                                            }
                                            _ => continue,
                                        };
                                        let percentile = match &args[1] {
                                            Expression::Literal(Literal::Float(f)) => *f,
                                            Expression::Literal(Literal::Integer(i)) => *i as f64,
                                            _ => continue,
                                        };
                                        aggregations.push(Aggregation::PercentileCont {
                                            column,
                                            alias: item
                                                .alias
                                                .clone()
                                                .unwrap_or_else(|| "percentileCont".to_string()),
                                            percentile,
                                        });
                                    }
                                }
                                "percentiledisc" => {
                                    has_aggregation = true;
                                    if args.len() >= 2 {
                                        let column = match &args[0] {
                                            Expression::Variable(var) => var.clone(),
                                            Expression::PropertyAccess { variable, property } => {
                                                format!("{}.{}", variable, property)
                                            }
                                            _ => continue,
                                        };
                                        let percentile = match &args[1] {
                                            Expression::Literal(Literal::Float(f)) => *f,
                                            Expression::Literal(Literal::Integer(i)) => *i as f64,
                                            _ => continue,
                                        };
                                        aggregations.push(Aggregation::PercentileDisc {
                                            column,
                                            alias: item
                                                .alias
                                                .clone()
                                                .unwrap_or_else(|| "percentileDisc".to_string()),
                                            percentile,
                                        });
                                    }
                                }
                                "collect" => {
                                    has_aggregation = true;
                                    let distinct = args.first().is_some_and(|arg| {
                                        if let Expression::Variable(v) = arg {
                                            v == "__DISTINCT__"
                                        } else {
                                            false
                                        }
                                    });

                                    // Get the actual argument (skip __DISTINCT__ if present)
                                    let actual_arg = if distinct && args.len() > 1 {
                                        Some(&args[1])
                                    } else if !distinct && !args.is_empty() {
                                        Some(&args[0])
                                    } else {
                                        None
                                    };

                                    if let Some(arg) = actual_arg {
                                        let column = match arg {
                                            Expression::Variable(var) => var.clone(),
                                            Expression::PropertyAccess { variable, property } => {
                                                format!("{}.{}", variable, property)
                                            }
                                            // For any other expression (including BinaryOp like x * 2),
                                            // create a projection item first so the expression is evaluated
                                            // before collect aggregates the results
                                            _ => {
                                                let alias =
                                                    format!("__collect_arg_{}", aggregations.len());
                                                projection_items.push(ProjectionItem {
                                                    alias: alias.clone(),
                                                    expression: arg.clone(),
                                                });
                                                alias
                                            }
                                        };
                                        aggregations.push(Aggregation::Collect {
                                            column,
                                            alias: item
                                                .alias
                                                .clone()
                                                .unwrap_or_else(|| "collect".to_string()),
                                            distinct,
                                        });
                                    }
                                }
                                _ => {
                                    // Not an aggregate function, treat as regular projection
                                    projection_items.push(ProjectionItem {
                                        alias: item.alias.clone().unwrap_or_else(|| {
                                            self.expression_to_string(&item.expression)
                                                .unwrap_or_default()
                                        }),
                                        expression: item.expression.clone(),
                                    });
                                }
                            }
                        }
                        _ => {
                            // Non-aggregate expression
                            projection_items.push(ProjectionItem {
                                alias: item.alias.clone().unwrap_or_else(|| {
                                    self.expression_to_string(&item.expression)
                                        .unwrap_or_default()
                                }),
                                expression: item.expression.clone(),
                            });
                        }
                    }
                }

                if has_aggregation {
                    // Add Project operator if needed (for literals in aggregations)
                    if !projection_items.is_empty() {
                        operators.push(Operator::Project {
                            items: projection_items.clone(),
                        });
                    }
                    // Preserve the written clause order across the
                    // aggregate's `[group keys..., aggs...]` assembly (G4);
                    // same alias derivation as the projection items above.
                    let output_order: Vec<String> = return_items
                        .iter()
                        .map(|item| {
                            item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            })
                        })
                        .collect();
                    // Add Aggregate operator with projection items
                    operators.push(Operator::Aggregate {
                        group_by: group_by_columns,
                        aggregations,
                        projection_items: if projection_items.is_empty() {
                            None
                        } else {
                            Some(projection_items)
                        },
                        output_order: Some(output_order),
                        source: None,
                        streaming_optimized: false,
                        push_down_optimized: false,
                    });

                    // If WITH had a WHERE clause with aggregation, add Filter after Aggregate
                    if let Some(ref where_expression) = with_aggregation_where {
                        let filter_str = self.expression_to_string(where_expression)?;
                        tracing::debug!(
                            "WITH aggregation WHERE: Adding Filter '{}' after Aggregate",
                            filter_str
                        );
                        operators.push(Operator::Filter {
                            predicate: filter_str,
                        });
                    }
                } else {
                    // Regular projection (no aggregations)
                    let projection_items: Vec<ProjectionItem> = return_items
                        .iter()
                        .map(|item| ProjectionItem {
                            alias: item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            }),
                            expression: item.expression.clone(),
                        })
                        .collect();
                    operators.push(Operator::Project {
                        items: projection_items,
                    });

                    // Add DISTINCT operator if specified
                    if return_distinct {
                        let distinct_columns: Vec<String> = return_items
                            .iter()
                            .map(|item| {
                                item.alias.clone().unwrap_or_else(|| {
                                    self.expression_to_string(&item.expression)
                                        .unwrap_or_default()
                                })
                            })
                            .collect();
                        operators.push(Operator::Distinct {
                            columns: distinct_columns,
                        });
                    }
                }
            }

            if let Some(limit) = limit_count {
                operators.push(Operator::Limit { count: limit });
            }
        } else if operators
            .iter()
            .any(|op| matches!(op, Operator::CallProcedure { .. }))
        {
            // CALL procedure standalone - it will produce its own columns/rows
            // If there's a RETURN after CALL, we need to project the YIELD columns
            // But if CALL is standalone with YIELD, the executor handles it
            // Just ensure we have operators (CALL procedure should already be added)
            if operators.is_empty() {
                return Err(Error::CypherSyntax(
                    "CALL procedure query must have at least one operator".to_string(),
                ));
            }

            // Apply LIMIT if specified
            if let Some(limit) = limit_count {
                operators.push(Operator::Limit { count: limit });
            }
        }

        // phase6_spatial-planner-seek §2 + §3 — try to rewrite the
        // operator pipeline to use `SpatialSeek` when an R-tree
        // index covers the predicate. The rewriter is a no-op when
        // no registry is installed or when no shape matches; the
        // cost-based picker (§3) only swaps in the seek when its
        // estimated cost is below the legacy `NodeByLabel + Filter`.
        let operators = self.try_rewrite_spatial_seek(query, operators);

        // Cache the planned operators for future use
        // Estimate cost using the improved cost model
        let estimated_cost = self
            .estimate_plan_cost(&operators)
            .unwrap_or(operators.len() as f64);
        self.plan_cache
            .put(query_hash, operators.clone(), estimated_cost);

        Ok(operators)
    }
}
