use super::parser::{
    BinaryOperator, Clause, CypherQuery, Expression, Literal, Pattern, PatternElement, QueryHint,
    RelationshipDirection, ReturnItem, SortDirection, UnaryOperator,
};
use super::{Aggregation, Direction, JoinType, Operator, ProjectionItem};
use crate::cache::relationship_index::RelationshipTraversalStats;
use crate::catalog::Catalog;
use crate::index::{KnnIndex, LabelIndex};
use crate::{Error, Result};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

/// Cached query plan with metadata
#[derive(Debug, Clone)]
pub struct CachedQueryPlan {
    /// The planned operators
    pub operators: Vec<Operator>,
    /// When this plan was cached
    pub cached_at: Instant,
    /// How many times this plan has been used
    pub access_count: u64,
    /// Estimated cost of the plan
    pub estimated_cost: f64,
}

/// Query plan cache for optimizing repeated queries
#[derive(Debug)]
pub struct QueryPlanCache {
    /// Cache of plans by query hash
    plans: HashMap<u64, CachedQueryPlan>,
    /// Maximum number of cached plans
    max_plans: usize,
    /// Time-to-live for cached plans
    ttl: Duration,
    /// Statistics
    stats: QueryPlanCacheStats,
}

/// Statistics for query plan cache
#[derive(Debug, Clone, Default)]
pub struct QueryPlanCacheStats {
    /// Total cache lookups
    pub lookups: u64,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Plans evicted due to size limits
    pub evictions: u64,
    /// Plans evicted due to TTL expiration
    pub expirations: u64,
    /// Total plans currently cached
    pub cached_plans: u64,
    /// Total plan reuse count (sum of access_count for all cached plans)
    pub total_reuse_count: u64,
    /// Average reuse count per plan
    pub avg_reuse_per_plan: f64,
    /// Total memory used by cached plans (estimated)
    pub memory_usage: usize,
}

/// Detailed plan reuse statistics
#[derive(Debug, Clone)]
pub struct PlanReuseStats {
    /// Total number of cached plans
    pub total_plans: u64,
    /// Number of plans used only once
    pub single_use_plans: u64,
    /// Number of plans used multiple times
    pub multi_use_plans: u64,
    /// Maximum reuse count for any plan
    pub max_reuse_count: u64,
    /// Average reuse count across all plans
    pub avg_reuse_count: f64,
    /// Plans with reuse count in different ranges
    pub reuse_distribution: HashMap<String, u64>,
}

/// Cached aggregation result
#[derive(Debug, Clone)]
pub struct CachedAggregationResult {
    /// The aggregation key (group by columns + aggregation expressions)
    key: String,
    /// The computed result
    result: serde_json::Value,
    /// When this result was cached
    cached_at: Instant,
    /// How many times this result has been used
    access_count: u64,
    /// Time-to-live for this cached result
    ttl: Duration,
}

/// Aggregation result cache
#[derive(Debug)]
pub struct AggregationCache {
    /// Cache of aggregation results
    cache: HashMap<String, CachedAggregationResult>,
    /// Maximum number of cached results
    max_results: usize,
    /// Default TTL for cached results
    default_ttl: Duration,
}

impl AggregationCache {
    /// Create a new aggregation cache
    pub fn new(max_results: usize, default_ttl: Duration) -> Self {
        Self {
            cache: HashMap::new(),
            max_results,
            default_ttl,
        }
    }

    /// Get a cached aggregation result
    pub fn get(&mut self, key: &str) -> Option<&serde_json::Value> {
        if let Some(result) = self.cache.get(key) {
            // Check if expired
            if result.cached_at.elapsed() > result.ttl {
                self.cache.remove(key);
                return None;
            }

            // Update access count
            if let Some(result) = self.cache.get_mut(key) {
                result.access_count += 1;
            }

            Some(&self.cache[key].result)
        } else {
            None
        }
    }

    /// Store an aggregation result in cache
    pub fn put(&mut self, key: String, result: serde_json::Value) {
        // Evict if cache is full (simple LRU)
        if self.cache.len() >= self.max_results {
            if let Some(oldest_key) = self
                .cache
                .iter()
                .min_by_key(|(_, result)| result.cached_at)
                .map(|(key, _)| key.clone())
            {
                self.cache.remove(&oldest_key);
            }
        }

        let cached_result = CachedAggregationResult {
            key: key.clone(),
            result,
            cached_at: Instant::now(),
            access_count: 0,
            ttl: self.default_ttl,
        };

        self.cache.insert(key, cached_result);
    }

    /// Clean expired entries
    pub fn clean_expired(&mut self) {
        let mut expired = Vec::new();
        for (key, result) in &self.cache {
            if result.cached_at.elapsed() > result.ttl {
                expired.push(key.clone());
            }
        }

        for key in expired {
            self.cache.remove(&key);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> AggregationCacheStats {
        let mut total_accesses = 0u64;
        let mut max_accesses = 0u64;

        for result in self.cache.values() {
            total_accesses += result.access_count;
            max_accesses = max_accesses.max(result.access_count);
        }

        AggregationCacheStats {
            total_results: self.cache.len() as u64,
            total_accesses,
            avg_accesses_per_result: if self.cache.is_empty() {
                0.0
            } else {
                total_accesses as f64 / self.cache.len() as f64
            },
            max_accesses,
        }
    }
}

/// Statistics for aggregation cache
#[derive(Debug, Clone)]
pub struct AggregationCacheStats {
    pub total_results: u64,
    pub total_accesses: u64,
    pub avg_accesses_per_result: f64,
    pub max_accesses: u64,
}

/// Query planner for optimizing Cypher execution
pub struct QueryPlanner<'a> {
    catalog: &'a Catalog,
    label_index: &'a LabelIndex,
    knn_index: &'a KnnIndex,
    /// Query plan cache for performance optimization
    plan_cache: QueryPlanCache,
    /// Aggregation result cache for intermediate results
    aggregation_cache: AggregationCache,
}

impl QueryPlanCache {
    /// Create a new query plan cache
    pub fn new(max_plans: usize, ttl: Duration) -> Self {
        Self {
            plans: HashMap::new(),
            max_plans,
            ttl,
            stats: QueryPlanCacheStats::default(),
        }
    }

    /// Get a cached plan by query hash
    pub fn get(&mut self, query_hash: u64) -> Option<&CachedQueryPlan> {
        self.stats.lookups += 1;

        if let Some(plan) = self.plans.get(&query_hash) {
            // Check if plan has expired
            if plan.cached_at.elapsed() > self.ttl {
                // Remove expired plan
                self.plans.remove(&query_hash);
                self.stats.expirations += 1;
                self.stats.misses += 1;
                return None;
            }

            // Update access statistics (need to get mutable reference again)
            if let Some(plan) = self.plans.get_mut(&query_hash) {
                plan.access_count += 1;
            }
            self.stats.hits += 1;
            self.plans.get(&query_hash)
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Store a plan in cache
    pub fn put(&mut self, query_hash: u64, operators: Vec<Operator>, estimated_cost: f64) {
        // Evict if cache is full (simple LRU-like behavior)
        if self.plans.len() >= self.max_plans {
            // Remove oldest plan (simple implementation)
            if let Some(oldest_hash) = self
                .plans
                .iter()
                .min_by_key(|(_, plan)| plan.cached_at)
                .map(|(hash, _)| *hash)
            {
                self.plans.remove(&oldest_hash);
                self.stats.evictions += 1;
            }
        }

        let cached_plan = CachedQueryPlan {
            operators,
            cached_at: Instant::now(),
            access_count: 0,
            estimated_cost,
        };

        self.plans.insert(query_hash, cached_plan);
        self.update_stats();
    }

    /// Get cache statistics
    pub fn stats(&self) -> &QueryPlanCacheStats {
        &self.stats
    }

    /// Clear all cached plans
    pub fn clear(&mut self) {
        self.plans.clear();
        self.stats = QueryPlanCacheStats::default();
    }

    /// Clean expired plans
    pub fn clean_expired(&mut self) {
        let mut expired = Vec::new();
        for (hash, plan) in &self.plans {
            if plan.cached_at.elapsed() > self.ttl {
                expired.push(*hash);
            }
        }

        for hash in expired {
            self.plans.remove(&hash);
            self.stats.expirations += 1;
        }

        self.update_stats();
    }

    /// Update computed statistics
    fn update_stats(&mut self) {
        self.stats.cached_plans = self.plans.len() as u64;

        let mut total_reuse = 0u64;
        let mut max_reuse = 0u64;
        let mut memory_usage = 0usize;

        for plan in self.plans.values() {
            total_reuse += plan.access_count;
            max_reuse = max_reuse.max(plan.access_count);
            // Rough memory estimation: operators + overhead
            memory_usage += std::mem::size_of_val(&plan.operators) + 100;
        }

        self.stats.total_reuse_count = total_reuse;
        self.stats.avg_reuse_per_plan = if self.stats.cached_plans > 0 {
            total_reuse as f64 / self.stats.cached_plans as f64
        } else {
            0.0
        };
        self.stats.memory_usage = memory_usage;
    }

    /// Get detailed plan reuse statistics
    pub fn plan_reuse_stats(&self) -> PlanReuseStats {
        let mut single_use = 0u64;
        let mut multi_use = 0u64;
        let mut max_reuse = 0u64;
        let mut total_reuse = 0u64;
        let mut distribution = HashMap::new();

        for plan in self.plans.values() {
            total_reuse += plan.access_count;
            max_reuse = max_reuse.max(plan.access_count);

            if plan.access_count == 1 {
                single_use += 1;
            } else if plan.access_count > 1 {
                multi_use += 1;
            }

            // Build reuse distribution
            let range = match plan.access_count {
                0 => unreachable!(),
                1 => "1".to_string(),
                2..=5 => "2-5".to_string(),
                6..=10 => "6-10".to_string(),
                11..=50 => "11-50".to_string(),
                51..=100 => "51-100".to_string(),
                _ => "100+".to_string(),
            };
            *distribution.entry(range).or_insert(0) += 1;
        }

        let avg_reuse = if self.plans.is_empty() {
            0.0
        } else {
            total_reuse as f64 / self.plans.len() as f64
        };

        PlanReuseStats {
            total_plans: self.plans.len() as u64,
            single_use_plans: single_use,
            multi_use_plans: multi_use,
            max_reuse_count: max_reuse,
            avg_reuse_count: avg_reuse,
            reuse_distribution: distribution,
        }
    }
}

impl<'a> QueryPlanner<'a> {
    /// Create a new query planner
    pub fn new(catalog: &'a Catalog, label_index: &'a LabelIndex, knn_index: &'a KnnIndex) -> Self {
        Self {
            catalog,
            label_index,
            knn_index,
            plan_cache: QueryPlanCache::new(1000, Duration::from_secs(300)), // 1000 plans, 5min TTL
            aggregation_cache: AggregationCache::new(500, Duration::from_secs(180)), // 500 results, 3min TTL
        }
    }

    /// Generate a hash for query caching based on query structure
    fn hash_query(&self, query: &CypherQuery) -> u64 {
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
                        union_clause.union_type == super::parser::UnionType::Distinct
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
                };
                let right_query = CypherQuery {
                    clauses: right_clauses,
                    params: query.params.clone(),
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
                    plan_cache: QueryPlanCache::new(0, std::time::Duration::from_secs(0)), // Empty cache
                    aggregation_cache: AggregationCache::new(
                        100,
                        std::time::Duration::from_secs(3600),
                    ),
                };
                let left_operators = temp_planner.plan_query(&left_query)?;
                let right_operators = temp_planner.plan_query(&right_query)?;

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
        let mut where_clauses = Vec::new();
        let mut return_items = Vec::new();
        let mut limit_count = None;
        let mut return_distinct = false;
        let mut unwind_operators = Vec::new(); // Collect UNWIND to insert after MATCH
        let mut create_patterns = Vec::new(); // Collect CREATE to insert after MATCH
        let mut match_hints = Vec::new(); // Collect hints from MATCH clauses
        let mut order_by_clause: Option<(Vec<String>, Vec<bool>)> = None; // Collect ORDER BY to add after projection

        for clause in &query.clauses {
            match clause {
                Clause::Match(match_clause) => {
                    // For OPTIONAL MATCH, we need to handle NULL values for unmatched patterns
                    // Store pattern with optional flag for later handling in executor
                    patterns.push(match_clause.pattern.clone());
                    if let Some(where_clause) = &match_clause.where_clause {
                        where_clauses.push(where_clause.expression.clone());
                    }
                    // Collect hints from first MATCH clause
                    if match_hints.is_empty() {
                        match_hints = match_clause.hints.clone();
                    }
                    // OPTIONAL MATCH is handled by executor as LEFT OUTER JOIN semantics
                    // For now, we just collect the patterns - executor will handle NULL values
                    // Query hints are stored in match_clause.hints and will be used during planning
                }
                Clause::Create(create_clause) => {
                    // Collect CREATE patterns to add AFTER MATCH operators
                    create_patterns.push(create_clause.pattern.clone());
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
                    patterns.push(merge_clause.pattern.clone());
                    // MERGE is handled as match-or-create
                    // Store pattern for executor to handle
                }
                Clause::Where(where_clause) => {
                    where_clauses.push(where_clause.expression.clone());
                }
                Clause::With(with_clause) => {
                    // WITH is similar to RETURN but for intermediate results
                    // Store the WITH items as new projection columns
                    return_items = with_clause.items.clone();
                    // Apply WHERE filtering if present in WITH clause
                    if let Some(where_clause) = &with_clause.where_clause {
                        where_clauses.push(where_clause.expression.clone());
                    }
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
                    return_items = return_clause.items.clone();
                    return_distinct = return_clause.distinct;
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
                &match_hints,
                &order_by_clause,
                &mut operators,
            )?;
        }

        // Add CREATE operators AFTER MATCH operators
        // This ensures CREATE runs after all nodes are matched
        for create_pattern in create_patterns {
            operators.push(Operator::Create {
                pattern: create_pattern,
            });
        }

        if patterns.is_empty() && (!return_items.is_empty() || !unwind_operators.is_empty()) {
            // No patterns but have RETURN or UNWIND - check for aggregations first
            // This handles cases like: RETURN count(*), RETURN sum(1), etc.
            operators.extend(unwind_operators);

            // Add filter operators for WHERE clauses (when there are no patterns)
            // This handles cases like: RETURN 42 WHERE false, RETURN 5 WHERE 5 > 10, etc.
            for where_clause in &where_clauses {
                let predicate = self.expression_to_string(where_clause)?;
                operators.push(Operator::Filter { predicate });
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
                                            Expression::Literal(_) => {
                                                let alias =
                                                    format!("__collect_arg_{}", aggregations.len());
                                                projection_items.push(ProjectionItem {
                                                    alias: alias.clone(),
                                                    expression: arg.clone(),
                                                });
                                                alias
                                            }
                                            _ => continue,
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
                    // Add Aggregate operator with projection items
                    operators.push(Operator::Aggregate {
                        group_by: group_by_columns,
                        aggregations,
                        projection_items: if projection_items.is_empty() {
                            None
                        } else {
                            Some(projection_items)
                        },
                        source: None,
                        streaming_optimized: false,
                        push_down_optimized: false,
                    });
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

        // Cache the planned operators for future use
        // Estimate cost using the improved cost model
        let estimated_cost = self
            .estimate_plan_cost(&operators)
            .unwrap_or(operators.len() as f64);
        self.plan_cache
            .put(query_hash, operators.clone(), estimated_cost);

        Ok(operators)
    }

    /// Plan execution strategy based on patterns and constraints
    #[allow(clippy::too_many_arguments)]
    fn plan_execution_strategy(
        &self,
        patterns: &[Pattern],
        where_clauses: &[Expression],
        return_items: &[ReturnItem],
        limit_count: Option<usize>,
        distinct: bool,
        unwind_operators: &[Operator],
        hints: &[QueryHint],
        order_by_clause: &Option<(Vec<String>, Vec<bool>)>,
        operators: &mut Vec<Operator>,
    ) -> Result<()> {
        // Process ALL patterns, not just the first one
        // Multiple patterns need Cartesian product (Join)
        let mut all_target_nodes = std::collections::HashSet::new();

        // Identify target nodes across all patterns
        // CRITICAL FIX: Include ALL nodes that are targets of relationships (Expand),
        // not just nodes without labels. Nodes that are targets of Expand will be populated
        // by the Expand operator and don't need a separate NodeByLabel.
        for pattern in patterns {
            for (idx, element) in pattern.elements.iter().enumerate() {
                if let PatternElement::Relationship(_) = element {
                    if idx + 1 < pattern.elements.len() {
                        if let PatternElement::Node(node) = &pattern.elements[idx + 1] {
                            if let Some(var) = &node.variable {
                                // CRITICAL: Add ALL target nodes, regardless of labels
                                // Nodes that are targets of Expand will be populated by Expand,
                                // so we shouldn't create NodeByLabel for them
                                all_target_nodes.insert(var.clone());
                            }
                        }
                    }
                }
            }
        }

        // Process the first pattern
        let start_pattern = self.select_start_pattern(patterns)?;

        // Add NodeByLabel operators for nodes in first pattern
        for element in &start_pattern.elements {
            if let PatternElement::Node(node) = element {
                if let Some(variable) = &node.variable {
                    // Skip if this node is a pure target without labels (will be populated by Expand)
                    if all_target_nodes.contains(variable) {
                        continue;
                    }

                    // Check for hints for this variable
                    let use_index_hint = hints.iter().find(|h| {
                        if let QueryHint::UsingIndex {
                            variable: hint_var, ..
                        } = h
                        {
                            hint_var == variable
                        } else {
                            false
                        }
                    });

                    let use_scan_hint = hints.iter().find(|h| {
                        if let QueryHint::UsingScan {
                            variable: hint_var, ..
                        } = h
                        {
                            hint_var == variable
                        } else {
                            false
                        }
                    });

                    if !node.labels.is_empty() {
                        // Use first label for initial scan
                        let first_label = &node.labels[0];
                        let label_id = self.catalog.get_or_create_label(first_label)?;

                        // Apply USING INDEX hint if present
                        if let Some(QueryHint::UsingIndex {
                            property: _property,
                            ..
                        }) = use_index_hint
                        {
                            // Force index usage for this property
                            // The executor will use property index lookup instead of label scan
                            operators.push(Operator::NodeByLabel {
                                label_id,
                                variable: variable.clone(),
                            });
                            // Add filter to use index (executor will detect property filter and use index)
                        } else if use_scan_hint.is_some() {
                            // USING SCAN hint - force label scan (already using NodeByLabel)
                            operators.push(Operator::NodeByLabel {
                                label_id,
                                variable: variable.clone(),
                            });
                        } else {
                            // Normal planning - use label scan
                            operators.push(Operator::NodeByLabel {
                                label_id,
                                variable: variable.clone(),
                            });
                        }

                        // Add filters for additional labels (multiple label intersection)
                        if node.labels.len() > 1 {
                            for additional_label in &node.labels[1..] {
                                // Create a filter that checks if node has this label
                                let filter_expr = format!("{}:{}", variable, additional_label);
                                operators.push(Operator::Filter {
                                    predicate: filter_expr,
                                });
                            }
                        }
                    } else {
                        // No label specified - need to scan all nodes
                        // Use AllNodesScan operator to scan all nodes efficiently
                        operators.push(Operator::AllNodesScan {
                            variable: variable.clone(),
                        });
                    }

                    // Add filters for inline properties: MATCH (n {property: value})
                    if let Some(property_map) = &node.properties {
                        for (prop_name, prop_value_expr) in &property_map.properties {
                            // Convert property value expression to string for filter
                            // Use single quotes for strings to match Cypher parser expectations
                            let value_str = match prop_value_expr {
                                Expression::Literal(lit) => match lit {
                                    Literal::String(s) => format!("'{}'", s),
                                    Literal::Integer(i) => i.to_string(),
                                    Literal::Float(f) => f.to_string(),
                                    Literal::Boolean(b) => b.to_string(),
                                    Literal::Null => "null".to_string(),
                                    Literal::Point(p) => p.to_string(),
                                },
                                _ => self.expression_to_string(prop_value_expr)?,
                            };
                            let filter_expr = format!("{}.{} = {}", variable, prop_name, value_str);
                            operators.push(Operator::Filter {
                                predicate: filter_expr,
                            });
                        }
                    }
                }
            }
        }

        // Add relationship traversal operators for first pattern
        self.add_relationship_operators(std::slice::from_ref(start_pattern), operators)?;

        // Process additional patterns (for comma-separated MATCH patterns like (p1:...), (p2:...))
        // Each additional pattern needs its own NodeByLabel + Filter operators
        for (pattern_idx, pattern) in patterns.iter().enumerate() {
            if pattern_idx == 0 {
                continue; // Skip first pattern, already processed
            }

            // Add NodeByLabel operators for nodes in this additional pattern
            for element in &pattern.elements {
                if let PatternElement::Node(node) = element {
                    if let Some(variable) = &node.variable {
                        if all_target_nodes.contains(variable) {
                            continue;
                        }

                        if !node.labels.is_empty() {
                            let first_label = &node.labels[0];
                            let label_id = self.catalog.get_or_create_label(first_label)?;
                            operators.push(Operator::NodeByLabel {
                                label_id,
                                variable: variable.clone(),
                            });

                            // Add filters for additional labels
                            if node.labels.len() > 1 {
                                for additional_label in &node.labels[1..] {
                                    let filter_expr = format!("{}:{}", variable, additional_label);
                                    operators.push(Operator::Filter {
                                        predicate: filter_expr,
                                    });
                                }
                            }
                        }

                        // Add filters for inline properties
                        if let Some(property_map) = &node.properties {
                            for (prop_name, prop_value_expr) in &property_map.properties {
                                // Use single quotes for strings to match Cypher parser expectations
                                let value_str = match prop_value_expr {
                                    Expression::Literal(lit) => match lit {
                                        Literal::String(s) => format!("'{}'", s),
                                        Literal::Integer(i) => i.to_string(),
                                        Literal::Float(f) => f.to_string(),
                                        Literal::Boolean(b) => b.to_string(),
                                        Literal::Null => "null".to_string(),
                                        Literal::Point(p) => p.to_string(),
                                    },
                                    _ => self.expression_to_string(prop_value_expr)?,
                                };
                                let filter_expr =
                                    format!("{}.{} = {}", variable, prop_name, value_str);
                                operators.push(Operator::Filter {
                                    predicate: filter_expr,
                                });
                            }
                        }
                    }
                }
            }

            // Add relationship operators for this pattern if any
            self.add_relationship_operators(std::slice::from_ref(pattern), operators)?;
        }

        // Add filter operators for WHERE clauses
        for where_clause in where_clauses {
            operators.push(Operator::Filter {
                predicate: self.expression_to_string(where_clause)?,
            });
        }

        // Capture order_by_clause reference before entering nested blocks to ensure it's accessible
        let order_by_clause_ref = order_by_clause.as_ref();

        // Add projection or aggregation operator for RETURN clause
        if !return_items.is_empty() {
            // Check if any return items contain aggregate functions
            let mut has_aggregation = false;
            let mut aggregations = Vec::new();
            let mut group_by_columns = Vec::new();

            let mut non_aggregate_aliases: Vec<String> = Vec::new();
            // Initialize projection_items early so we can add literal projections for aggregations
            let mut projection_items: Vec<ProjectionItem> = Vec::new();

            for item in return_items.iter() {
                // First, check if this expression contains any nested aggregations
                if self.contains_aggregation(&item.expression) {
                    has_aggregation = true;
                }

                match &item.expression {
                    Expression::FunctionCall { name, args } => {
                        let func_name = name.to_lowercase();
                        match func_name.as_str() {
                            "count" => {
                                has_aggregation = true;

                                // Check for DISTINCT marker
                                let mut distinct = false;
                                let mut real_args = args.clone();
                                if let Some(Expression::Variable(var)) = args.first() {
                                    if var == "__DISTINCT__" {
                                        distinct = true;
                                        real_args = args[1..].to_vec();
                                    }
                                }

                                let column = if real_args.is_empty() {
                                    None // COUNT(*) or COUNT(DISTINCT *)
                                } else if let Some(Expression::Variable(var)) = real_args.first() {
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
                                    // Handle literals by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        Expression::Literal(_) => {
                                            // For literals, create a projection item first
                                            let alias = format!("__sum_arg_{}", aggregations.len());
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
                                    // Handle literals by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        Expression::Literal(_) => {
                                            // For literals, create a projection item first
                                            let alias = format!("__avg_arg_{}", aggregations.len());
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
                                    // Handle literals by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        Expression::Literal(_) => {
                                            // For literals, create a projection item first
                                            let alias = format!("__min_arg_{}", aggregations.len());
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
                                    // Handle literals by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        Expression::Literal(_) => {
                                            // For literals, create a projection item first
                                            let alias = format!("__max_arg_{}", aggregations.len());
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
                                    // Handle literals by projecting them first
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        Expression::Literal(_) => {
                                            // For literals, create a projection item first
                                            let alias =
                                                format!("__collect_arg_{}", aggregations.len());
                                            projection_items.push(ProjectionItem {
                                                alias: alias.clone(),
                                                expression: arg.clone(),
                                            });
                                            alias
                                        }
                                        _ => continue,
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
                                // Not an aggregate function, but might contain nested aggregations
                                // Check if any argument contains an aggregation
                                let mut has_nested_agg = false;
                                let mut temp_agg_alias: Option<String> = None;

                                for arg in args {
                                    if self.contains_aggregation(&arg) {
                                        has_nested_agg = true;
                                        // Extract nested aggregation (e.g., collect() inside head())
                                        if let Expression::FunctionCall {
                                            name: nested_name,
                                            args: nested_args,
                                        } = arg
                                        {
                                            let nested_func = nested_name.to_lowercase();
                                            if nested_func == "collect" {
                                                let distinct =
                                                    nested_args.first().is_some_and(|arg| {
                                                        if let Expression::Variable(v) = arg {
                                                            v == "__DISTINCT__"
                                                        } else {
                                                            false
                                                        }
                                                    });

                                                let actual_arg =
                                                    if distinct && nested_args.len() > 1 {
                                                        Some(&nested_args[1])
                                                    } else if !distinct && !nested_args.is_empty() {
                                                        Some(&nested_args[0])
                                                    } else {
                                                        None
                                                    };

                                                if let Some(arg) = actual_arg {
                                                    let column = match arg {
                                                        Expression::Variable(var) => var.clone(),
                                                        Expression::PropertyAccess {
                                                            variable,
                                                            property,
                                                        } => {
                                                            format!("{}.{}", variable, property)
                                                        }
                                                        Expression::Literal(_) => {
                                                            let alias = format!(
                                                                "__collect_arg_{}",
                                                                aggregations.len()
                                                            );
                                                            projection_items.push(ProjectionItem {
                                                                alias: alias.clone(),
                                                                expression: arg.clone(),
                                                            });
                                                            alias
                                                        }
                                                        _ => continue,
                                                    };
                                                    // Create temporary alias for the aggregation result
                                                    let temp_alias =
                                                        format!("__agg_{}", aggregations.len());
                                                    temp_agg_alias = Some(temp_alias.clone());
                                                    aggregations.push(Aggregation::Collect {
                                                        column,
                                                        alias: temp_alias,
                                                        distinct,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }

                                if has_nested_agg {
                                    // Don't add to non_aggregate_aliases - we'll handle this in post-aggregation projection
                                    // The nested aggregation will be extracted and handled separately
                                } else {
                                    // Not an aggregate function and no nested aggregations, treat as regular column for GROUP BY
                                    let alias = item.alias.clone().unwrap_or_else(|| {
                                        self.expression_to_string(&item.expression)
                                            .unwrap_or_default()
                                    });
                                    non_aggregate_aliases.push(alias);
                                }
                            }
                        }
                    }
                    _ => {
                        // Non-aggregate expression, add to GROUP BY if there are aggregations
                        let alias = item.alias.clone().unwrap_or_else(|| {
                            self.expression_to_string(&item.expression)
                                .unwrap_or_default()
                        });
                        non_aggregate_aliases.push(alias);
                    }
                }
            }

            if has_aggregation {
                let mut required_columns: HashSet<String> = HashSet::new();

                if group_by_columns.is_empty() {
                    group_by_columns = non_aggregate_aliases.clone();
                } else {
                    for alias in &non_aggregate_aliases {
                        if !group_by_columns.contains(alias) {
                            group_by_columns.push(alias.clone());
                        }
                    }
                }

                // CRITICAL FIX: Add projection items for all GROUP BY columns
                // This ensures that Project operator creates columns with correct aliases
                // before Aggregate tries to group by them
                for col in &group_by_columns {
                    // Check if this column is already in projection_items
                    if !projection_items.iter().any(|item| item.alias == *col) {
                        // Try to find the corresponding return item to get the expression
                        let mut found = false;
                        for item in return_items {
                            let alias = item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            });
                            if alias == *col {
                                // Found the matching return item, add it to projection_items
                                projection_items.push(ProjectionItem {
                                    alias: col.clone(),
                                    expression: item.expression.clone(),
                                });
                                found = true;
                                break;
                            }
                        }
                        // If not found in return_items, create a projection item from the column name
                        if !found {
                            let expression = if col.contains('.') {
                                let parts: Vec<&str> = col.split('.').collect();
                                if parts.len() == 2 {
                                    Expression::PropertyAccess {
                                        variable: parts[0].to_string(),
                                        property: parts[1].to_string(),
                                    }
                                } else {
                                    Expression::Variable(col.clone())
                                }
                            } else {
                                Expression::Variable(col.clone())
                            };
                            projection_items.push(ProjectionItem {
                                alias: col.clone(),
                                expression,
                            });
                        }
                    }
                }

                for item in return_items {
                    match &item.expression {
                        Expression::FunctionCall { name, args } => {
                            let func_name = name.to_lowercase();
                            match func_name.as_str() {
                                "count" | "sum" | "avg" | "min" | "max" | "collect" => {
                                    // Skip DISTINCT marker if present
                                    let real_args =
                                        if let Some(Expression::Variable(var)) = args.first() {
                                            if var == "__DISTINCT__" {
                                                &args[1..]
                                            } else {
                                                args.as_slice()
                                            }
                                        } else {
                                            args.as_slice()
                                        };

                                    if let Some(arg) = real_args.first() {
                                        match arg {
                                            Expression::Variable(var) => {
                                                required_columns.insert(var.clone());
                                            }
                                            Expression::PropertyAccess { variable, property } => {
                                                required_columns
                                                    .insert(format!("{}.{}", variable, property));
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {
                                    // Check if this function contains nested aggregations
                                    // If so, don't add to projection_items here - it will be handled in post-aggregation projection
                                    if !self.contains_aggregation(&item.expression) {
                                        let alias = item.alias.clone().unwrap_or_else(|| {
                                            self.expression_to_string(&item.expression)
                                                .unwrap_or_default()
                                        });
                                        projection_items.push(ProjectionItem {
                                            alias,
                                            expression: item.expression.clone(),
                                        });
                                    }
                                }
                            }
                        }
                        _ => {
                            let alias = item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            });
                            projection_items.push(ProjectionItem {
                                alias,
                                expression: item.expression.clone(),
                            });
                        }
                    }
                }

                for column in required_columns {
                    if !projection_items.iter().any(|item| item.alias == column) {
                        let expression = if column.contains('.') {
                            let parts: Vec<&str> = column.split('.').collect();
                            if parts.len() == 2 {
                                Expression::PropertyAccess {
                                    variable: parts[0].to_string(),
                                    property: parts[1].to_string(),
                                }
                            } else {
                                Expression::Variable(column.clone())
                            }
                        } else {
                            Expression::Variable(column.clone())
                        };

                        projection_items.push(ProjectionItem {
                            alias: column.clone(),
                            expression,
                        });
                    }
                }

                if !projection_items.is_empty() {
                    operators.push(Operator::Project {
                        items: projection_items.clone(),
                    });
                }

                // Insert UNWIND operators before aggregation
                for op in unwind_operators {
                    operators.push(op.clone());
                }

                let aggregations_clone = aggregations.clone();
                operators.push(Operator::Aggregate {
                    group_by: group_by_columns,
                    aggregations,
                    projection_items: if projection_items.is_empty() {
                        None
                    } else {
                        Some(projection_items)
                    },
                    source: None,
                    streaming_optimized: false,
                    push_down_optimized: false,
                });

                // After aggregation, apply any non-aggregate functions that wrap aggregations
                // (e.g., head(collect(...)), tail(collect(...)), reverse(collect(...)))
                let mut post_agg_projection_items = Vec::new();
                for item in return_items {
                    if let Expression::FunctionCall { name, .. } = &item.expression {
                        let func_name = name.to_lowercase();
                        // Check if this is a non-aggregate function that contains nested aggregations
                        if !matches!(
                            func_name.as_str(),
                            "count" | "sum" | "avg" | "min" | "max" | "collect"
                        ) && self.contains_aggregation(&item.expression)
                        {
                            // Replace nested aggregations with variable references
                            let modified_expr = self
                                .replace_nested_aggregations(&item.expression, &aggregations_clone);
                            post_agg_projection_items.push(ProjectionItem {
                                alias: item.alias.clone().unwrap_or_else(|| {
                                    self.expression_to_string(&item.expression)
                                        .unwrap_or_default()
                                }),
                                expression: modified_expr,
                            });
                        }
                    }
                }

                if !post_agg_projection_items.is_empty() {
                    operators.push(Operator::Project {
                        items: post_agg_projection_items,
                    });
                }
            } else {
                // Insert UNWIND operators before final projection
                for op in unwind_operators {
                    operators.push(op.clone());
                }

                // Regular projection
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
                if distinct {
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

                // Add ORDER BY after DISTINCT if UNWIND is present (ORDER BY must come after DISTINCT)
                // This ensures correct order: UNWIND → Project → DISTINCT → ORDER BY → LIMIT
                if !unwind_operators.is_empty() {
                    if let Some((columns, ascending)) = order_by_clause_ref {
                        // Build a map of expression -> alias from return_items for resolution
                        let mut expression_to_alias = std::collections::HashMap::new();
                        for item in return_items.iter() {
                            let expr_str = self
                                .expression_to_string(&item.expression)
                                .unwrap_or_default();
                            let alias = item.alias.clone().unwrap_or_else(|| expr_str.clone());
                            expression_to_alias.insert(expr_str, alias);
                        }

                        // Resolve ORDER BY column names to aliases
                        let resolved_columns: Vec<String> = columns
                            .iter()
                            .map(|col| {
                                // Try to resolve to alias, otherwise use as-is
                                expression_to_alias
                                    .get(col)
                                    .cloned()
                                    .unwrap_or_else(|| col.clone())
                            })
                            .collect();

                        // Add ORDER BY right after DISTINCT (which was just added above)
                        operators.push(Operator::Sort {
                            columns: resolved_columns,
                            ascending: ascending.clone(),
                        });
                    }
                }
            }
        }

        // Add ORDER BY operator (Sort) AFTER projection/aggregation but BEFORE limit
        // This handles ORDER BY for queries WITHOUT UNWIND + DISTINCT
        // (UNWIND + DISTINCT case was already handled above)
        // Check if ORDER BY was already added (for UNWIND queries)
        let order_by_added =
            !unwind_operators.is_empty() && distinct && order_by_clause_ref.is_some();

        if !order_by_added {
            if let Some((columns, ascending)) = order_by_clause_ref {
                // Build a map of expression -> alias from return_items for resolution
                let mut expression_to_alias = std::collections::HashMap::new();
                for item in return_items.iter() {
                    let expr_str = self
                        .expression_to_string(&item.expression)
                        .unwrap_or_default();
                    let alias = item.alias.clone().unwrap_or_else(|| expr_str.clone());
                    expression_to_alias.insert(expr_str, alias);
                }

                // Resolve ORDER BY column names to aliases
                let resolved_columns: Vec<String> = columns
                    .iter()
                    .map(|col| {
                        // Try to resolve to alias, otherwise use as-is
                        expression_to_alias
                            .get(col)
                            .cloned()
                            .unwrap_or_else(|| col.clone())
                    })
                    .collect();

                // Find where to insert Sort (before Limit if exists)
                let limit_pos = operators
                    .iter()
                    .position(|op| matches!(op, Operator::Limit { .. }));

                let sort_op = Operator::Sort {
                    columns: resolved_columns,
                    ascending: ascending.clone(),
                };

                if let Some(pos) = limit_pos {
                    // Insert before Limit
                    operators.insert(pos, sort_op);
                } else {
                    // Add at the end
                    operators.push(sort_op);
                }
            }
        }

        // Add limit operator if specified
        if let Some(count) = limit_count {
            operators.push(Operator::Limit { count });
        }

        Ok(())
    }

    /// Select the most selective pattern to start execution
    fn select_start_pattern<'b>(&self, patterns: &'b [Pattern]) -> Result<&'b Pattern> {
        if patterns.is_empty() {
            return Err(Error::CypherSyntax(
                "No patterns found in query".to_string(),
            ));
        }

        // For MVP, just return the first pattern
        // In a full implementation, we would analyze selectivity
        Ok(&patterns[0])
    }

    /// Add relationship traversal operators
    fn add_relationship_operators(
        &self,
        patterns: &[Pattern],
        operators: &mut Vec<Operator>,
    ) -> Result<()> {
        let mut tmp_var_counter = 0;

        for pattern in patterns {
            // Track previous node variable for relationship expansion
            let mut prev_node_var: Option<String> = None;

            for (idx, element) in pattern.elements.iter().enumerate() {
                match element {
                    PatternElement::Node(node_pattern) => {
                        // Update previous node variable
                        // If node has explicit variable, use it
                        // Otherwise, keep the previous value (from last Expand's target_var)
                        if let Some(var) = &node_pattern.variable {
                            prev_node_var = Some(var.clone());
                        }
                        // Don't update prev_node_var if no variable - it should already be set by previous Expand
                    }
                    PatternElement::Relationship(rel) => {
                        let direction = match rel.direction {
                            RelationshipDirection::Outgoing => Direction::Outgoing,
                            RelationshipDirection::Incoming => Direction::Incoming,
                            RelationshipDirection::Both => Direction::Both,
                        };

                        // Determine source and target variables
                        let source_var = prev_node_var.clone().unwrap_or_default();

                        // Target will be the next node in the pattern
                        let target_var = if idx + 1 < pattern.elements.len() {
                            if let PatternElement::Node(next_node) = &pattern.elements[idx + 1] {
                                // If target node has explicit variable, use it
                                // Otherwise, generate temporary variable for chaining
                                next_node.variable.clone().unwrap_or_else(|| {
                                    let tmp_var = format!("__tmp_{}", tmp_var_counter);
                                    tmp_var_counter += 1;
                                    tmp_var
                                })
                            } else {
                                "".to_string()
                            }
                        } else {
                            "".to_string()
                        };

                        // Update prev_node_var to the target for next relationship
                        // This ensures multi-hop patterns chain correctly
                        prev_node_var = Some(target_var.clone());

                        // Get type_ids from relationship types (support multiple types like :TYPE1|TYPE2)
                        // CRITICAL FIX: Use get_or_create_type to ensure type exists even if not yet in catalog
                        // This handles cases where relationships are created but type lookup fails
                        let type_ids: Vec<u32> =
                            rel.types
                                .iter()
                                .filter_map(|type_name| {
                                    // Try get_type_id first (faster), fallback to get_or_create_type if not found
                                    self.catalog.get_type_id(type_name).ok().flatten().or_else(
                                        || {
                                            // Type might not exist yet - create it to ensure lookup works
                                            self.catalog.get_or_create_type(type_name).ok()
                                        },
                                    )
                                })
                                .collect();

                        // Check if this is a variable-length path (has quantifier)
                        if let Some(quantifier) = &rel.quantifier {
                            // Use VariableLengthPath operator for variable-length paths
                            // Check if pattern has a path variable assigned
                            let path_var = pattern.path_variable.clone().unwrap_or_default();
                            // For variable-length paths, only use first type for now
                            let type_id = type_ids.first().copied();
                            operators.push(Operator::VariableLengthPath {
                                type_id,
                                direction,
                                source_var,
                                target_var,
                                rel_var: rel.variable.clone().unwrap_or_default(),
                                path_var,
                                quantifier: quantifier.clone(),
                            });
                        } else {
                            // Use regular Expand operator for single-hop relationships
                            operators.push(Operator::Expand {
                                type_ids,
                                source_var,
                                target_var,
                                rel_var: rel.variable.clone().unwrap_or_default(),
                                direction,
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert expression to string representation
    fn expression_to_string(&self, expr: &Expression) -> Result<String> {
        match expr {
            Expression::Variable(name) => Ok(name.clone()),
            Expression::PropertyAccess { variable, property } => {
                Ok(format!("{}.{}", variable, property))
            }
            Expression::ArrayIndex { base, index } => {
                let base_str = self.expression_to_string(base)?;
                let index_str = self.expression_to_string(index)?;
                Ok(format!("{}[{}]", base_str, index_str))
            }
            Expression::Literal(literal) => match literal {
                // Use single quotes for strings to match Cypher parser expectations
                // This is critical for filter predicates to work correctly
                Literal::String(s) => Ok(format!("'{}'", s)),
                Literal::Integer(i) => Ok(i.to_string()),
                Literal::Float(f) => Ok(f.to_string()),
                Literal::Boolean(b) => Ok(b.to_string()),
                Literal::Null => Ok("NULL".to_string()),
                Literal::Point(p) => Ok(p.to_string()),
            },
            Expression::BinaryOp { left, op, right } => {
                let left_str = self.expression_to_string(left)?;
                let right_str = self.expression_to_string(right)?;
                let op_str = match op {
                    BinaryOperator::Equal => "=",
                    BinaryOperator::NotEqual => "!=",
                    BinaryOperator::LessThan => "<",
                    BinaryOperator::LessThanOrEqual => "<=",
                    BinaryOperator::GreaterThan => ">",
                    BinaryOperator::GreaterThanOrEqual => ">=",
                    BinaryOperator::And => "AND",
                    BinaryOperator::Or => "OR",
                    BinaryOperator::Add => "+",
                    BinaryOperator::Subtract => "-",
                    BinaryOperator::Multiply => "*",
                    BinaryOperator::Divide => "/",
                    BinaryOperator::In => "IN",
                    BinaryOperator::Contains => "CONTAINS",
                    BinaryOperator::StartsWith => "STARTS WITH",
                    BinaryOperator::EndsWith => "ENDS WITH",
                    BinaryOperator::RegexMatch => "=~",
                    BinaryOperator::Power => "^",
                    BinaryOperator::Modulo => "%",
                    _ => "?",
                };
                Ok(format!("{} {} {}", left_str, op_str, right_str))
            }
            Expression::Parameter(name) => Ok(format!("${}", name)),
            Expression::IsNull { expr, negated } => {
                let expr_str = self.expression_to_string(expr)?;
                if *negated {
                    Ok(format!("{} IS NOT NULL", expr_str))
                } else {
                    Ok(format!("{} IS NULL", expr_str))
                }
            }
            Expression::List(elements) => {
                let elem_strs: Result<Vec<String>> = elements
                    .iter()
                    .map(|e| self.expression_to_string(e))
                    .collect();
                Ok(format!("[{}]", elem_strs?.join(", ")))
            }
            Expression::Map(map) => {
                let mut pairs = Vec::new();
                for (key, value) in map {
                    let value_str = self.expression_to_string(value)?;
                    pairs.push(format!("{}: {}", key, value_str));
                }
                Ok(format!("{{{}}}", pairs.join(", ")))
            }
            Expression::FunctionCall { name, args } => {
                let arg_strs: Result<Vec<String>> =
                    args.iter().map(|a| self.expression_to_string(a)).collect();
                Ok(format!("{}({})", name, arg_strs?.join(", ")))
            }
            Expression::UnaryOp { op, operand } => {
                let operand_str = self.expression_to_string(operand)?;
                let op_str = match op {
                    UnaryOperator::Not => "NOT",
                    UnaryOperator::Minus => "-",
                    UnaryOperator::Plus => "+",
                };
                Ok(format!("{} {}", op_str, operand_str))
            }
            _ => Ok("?".to_string()),
        }
    }

    /// Check if an expression contains an aggregation function (recursively)
    fn contains_aggregation(&self, expr: &Expression) -> bool {
        match expr {
            Expression::FunctionCall { name, args } => {
                let func_name = name.to_lowercase();
                // Check if this is an aggregation function
                if matches!(
                    func_name.as_str(),
                    "count" | "sum" | "avg" | "min" | "max" | "collect"
                ) {
                    return true;
                }
                // Recursively check arguments
                for arg in args {
                    if self.contains_aggregation(arg) {
                        return true;
                    }
                }
                false
            }
            Expression::BinaryOp { left, right, .. } => {
                self.contains_aggregation(left) || self.contains_aggregation(right)
            }
            Expression::UnaryOp { operand, .. } => self.contains_aggregation(operand),
            Expression::List(elements) => elements.iter().any(|e| self.contains_aggregation(e)),
            Expression::Map(map) => map.values().any(|e| self.contains_aggregation(e)),
            Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                if let Some(input_expr) = input {
                    if self.contains_aggregation(input_expr) {
                        return true;
                    }
                }
                for when in when_clauses {
                    if self.contains_aggregation(&when.condition)
                        || self.contains_aggregation(&when.result)
                    {
                        return true;
                    }
                }
                if let Some(else_expr) = else_clause {
                    if self.contains_aggregation(else_expr) {
                        return true;
                    }
                }
                false
            }
            Expression::IsNull { expr, .. } => self.contains_aggregation(expr),
            Expression::ArrayIndex { base, index } => {
                self.contains_aggregation(base) || self.contains_aggregation(index)
            }
            _ => false,
        }
    }

    /// Replace nested aggregations in an expression with variable references
    fn replace_nested_aggregations(
        &self,
        expr: &Expression,
        aggregations: &[Aggregation],
    ) -> Expression {
        match expr {
            Expression::FunctionCall { name, args } => {
                let func_name = name.to_lowercase();
                // Check if this is a nested aggregation function
                if func_name == "collect" {
                    // Find matching aggregation by checking if the arguments match
                    for (idx, agg) in aggregations.iter().enumerate() {
                        if let Aggregation::Collect { column, .. } = agg {
                            // Check if this collect() matches the aggregation
                            if let Some(arg) = args.first() {
                                let matches = match arg {
                                    Expression::Variable(var) => var == column,
                                    Expression::PropertyAccess { variable, property } => {
                                        format!("{}.{}", variable, property) == *column
                                    }
                                    _ => false,
                                };
                                if matches {
                                    // Replace with variable reference to aggregation result
                                    let temp_alias = format!("__agg_{}", idx);
                                    return Expression::Variable(temp_alias);
                                }
                            }
                        }
                    }
                }

                // Recursively replace nested aggregations in arguments
                let new_args: Vec<Expression> = args
                    .iter()
                    .map(|arg| self.replace_nested_aggregations(arg, aggregations))
                    .collect();

                Expression::FunctionCall {
                    name: name.clone(),
                    args: new_args,
                }
            }
            Expression::BinaryOp { left, right, op } => Expression::BinaryOp {
                left: Box::new(self.replace_nested_aggregations(left, aggregations)),
                right: Box::new(self.replace_nested_aggregations(right, aggregations)),
                op: *op,
            },
            Expression::UnaryOp { op, operand } => Expression::UnaryOp {
                op: *op,
                operand: Box::new(self.replace_nested_aggregations(operand, aggregations)),
            },
            _ => expr.clone(),
        }
    }

    /// Estimate query cost for optimization
    pub fn estimate_cost(&self, operators: &[Operator]) -> Result<f64> {
        let mut total_cost = 0.0;

        for operator in operators {
            match operator {
                Operator::NodeByLabel { label_id, .. } => {
                    // Estimate cost based on label selectivity
                    let selectivity = self.estimate_label_selectivity(*label_id)?;
                    total_cost += 1000.0 * selectivity;
                }
                Operator::AllNodesScan { .. } => {
                    // Scanning all nodes is more expensive than label scan
                    // Assume full scan of all nodes
                    total_cost += 2000.0;
                }
                Operator::Filter { .. } => {
                    // Filter operations are relatively cheap
                    total_cost += 10.0;
                }
                Operator::Expand { .. } => {
                    // Relationship traversal is expensive
                    total_cost += 100.0;
                }
                Operator::Project { .. } => {
                    // Projection is cheap
                    total_cost += 1.0;
                }
                Operator::Limit { count } => {
                    // Limit reduces cost
                    total_cost *= (*count as f64) / 1000.0;
                }
                Operator::Sort { .. } => {
                    // Sorting is moderately expensive
                    total_cost += 50.0;
                }
                Operator::Aggregate { .. } => {
                    // Aggregation is moderately expensive
                    total_cost += 30.0;
                }
                Operator::Union { .. } => {
                    // Union is relatively cheap
                    total_cost += 20.0;
                }
                Operator::Join { .. } => {
                    // Join is expensive
                    total_cost += 200.0;
                }
                Operator::IndexScan { .. } => {
                    // Index scan is very cheap
                    total_cost += 5.0;
                }
                Operator::Distinct { .. } => {
                    // Distinct is moderately expensive
                    total_cost += 40.0;
                }
                Operator::HashJoin { .. } => {
                    // Hash join operations are moderately expensive
                    total_cost += 200.0;
                }
                Operator::Create { .. } => {
                    // CREATE operations are moderately expensive
                    total_cost += 50.0;
                }
                Operator::Delete { .. } => {
                    // DELETE operations are moderately expensive
                    total_cost += 40.0;
                }
                Operator::DetachDelete { .. } => {
                    // DETACH DELETE is more expensive (deletes relationships first)
                    total_cost += 60.0;
                }
                Operator::Unwind { .. } => {
                    // UNWIND expands list into rows - moderately cheap
                    total_cost += 15.0;
                }
                Operator::VariableLengthPath { .. } => {
                    // Variable-length paths are expensive (BFS traversal)
                    total_cost += 500.0;
                }
                Operator::CallProcedure { .. } => {
                    // Procedure calls are moderately expensive (depends on procedure)
                    total_cost += 200.0;
                }
                Operator::LoadCsv { .. } => {
                    // CSV loading is moderately expensive (file I/O)
                    total_cost += 50.0;
                }
                Operator::CreateIndex { .. } => {
                    // Index creation is cheap (metadata operation)
                    total_cost += 1.0;
                }
            }
        }

        Ok(total_cost)
    }

    /// Estimate selectivity of a label
    fn estimate_label_selectivity(&self, label_id: u32) -> Result<f64> {
        // Try to get real statistics from the label index
        let stats = self.label_index.get_stats();
        if stats.total_nodes > 0 {
            let label_count = self.label_index.get_nodes_with_labels(&[label_id])?.len();
            return Ok(label_count as f64 / stats.total_nodes as f64);
        }

        // Fallback to a conservative estimate if statistics unavailable
        Ok(0.1) // 10% selectivity
    }

    /// Estimate the total cost of an operator plan
    /// Advanced cost estimation with cardinality and I/O modeling
    pub fn estimate_plan_cost(&self, operators: &[Operator]) -> Result<f64> {
        let mut total_cost = 0.0;
        let mut current_cardinality = 1.0; // Estimated number of rows at current point

        for operator in operators {
            let (operator_cost, output_cardinality) =
                self.estimate_operator_cost(operator, current_cardinality)?;
            total_cost += operator_cost;
            current_cardinality = output_cardinality;
        }

        Ok(total_cost)
    }

    /// Estimate cost for a single operator with cardinality modeling
    fn estimate_operator_cost(
        &self,
        operator: &Operator,
        input_cardinality: f64,
    ) -> Result<(f64, f64)> {
        match operator {
            Operator::NodeByLabel { label_id, .. } => {
                // Get label statistics from catalog
                let label_stats = self.label_index.get_stats();
                let total_nodes = label_stats.total_nodes as f64;

                // Use average nodes per label as selectivity estimate
                let label_selectivity = if total_nodes > 0.0 {
                    label_stats.avg_nodes_per_label / total_nodes as f64
                } else {
                    1.0
                };

                let output_cardinality = total_nodes * label_selectivity;
                let io_cost = output_cardinality * 10.0; // I/O cost per node read
                let cpu_cost = output_cardinality * 2.0; // CPU cost per node processing

                Ok((io_cost + cpu_cost, output_cardinality))
            }

            Operator::AllNodesScan { .. } => {
                let label_stats = self.label_index.get_stats();
                let total_nodes = label_stats.total_nodes as f64;
                let io_cost = total_nodes * 15.0; // More expensive than indexed scan
                let cpu_cost = total_nodes * 1.0;

                Ok((io_cost + cpu_cost, total_nodes))
            }

            Operator::Filter { predicate, .. } => {
                // Estimate filter selectivity based on predicate type
                // For now, use a simple heuristic since predicate is a String
                let selectivity = 0.5; // Default 50% selectivity for filters
                let output_cardinality = input_cardinality * selectivity;

                // Filter is mostly CPU-bound
                let cpu_cost = input_cardinality * 5.0; // CPU cost per row filtered

                Ok((cpu_cost, output_cardinality))
            }

            Operator::Expand {
                type_ids,
                direction,
                ..
            } => {
                // Estimate relationship expansion cost
                let rel_stats = self.estimate_relationship_stats(&Some(type_ids.clone()))?;
                let avg_relationships_per_node = rel_stats.avg_relationships_per_node;

                let output_cardinality = input_cardinality * avg_relationships_per_node;

                // Relationship traversal involves both I/O and CPU
                let io_cost = input_cardinality * 20.0; // Reading relationship data
                let cpu_cost = output_cardinality * 3.0; // Processing each relationship

                Ok((io_cost + cpu_cost, output_cardinality))
            }

            Operator::Join {
                left,
                right,
                join_type,
                ..
            } => {
                let left_cardinality = self.estimate_plan_cost(&[*left.clone()])?;
                let right_cardinality = self.estimate_plan_cost(&[*right.clone()])?;

                let (join_cost, output_cardinality) = match join_type {
                    JoinType::Inner => {
                        // Estimate join selectivity (simplified)
                        let selectivity = 0.1; // Assume 10% of cartesian product
                        let cartesian = left_cardinality * right_cardinality;
                        let output_card = cartesian * selectivity;

                        // Hash join cost model
                        let build_cost = left_cardinality * 5.0; // Building hash table
                        let probe_cost = right_cardinality * 3.0; // Probing hash table

                        (build_cost + probe_cost, output_card)
                    }
                    JoinType::LeftOuter => {
                        // Left outer join preserves left side
                        let output_card = left_cardinality;
                        let cost = left_cardinality * 10.0 + right_cardinality * 5.0;
                        (cost, output_card)
                    }
                    _ => {
                        // Default to cartesian product cost
                        let output_card = left_cardinality * right_cardinality;
                        let cost = output_card * 2.0;
                        (cost, output_card)
                    }
                };

                Ok((join_cost, output_cardinality))
            }

            Operator::Union { left, right, .. } => {
                let left_cost = self.estimate_plan_cost(left)?;
                let right_cost = self.estimate_plan_cost(right)?;
                let left_card = self.estimate_operator_cardinality(left)?;
                let right_card = self.estimate_operator_cardinality(right)?;

                // Union cost is sum of both sides
                let total_cost = left_cost + right_cost;
                let output_cardinality = left_card + right_card; // Union removes duplicates conceptually

                Ok((total_cost, output_cardinality))
            }

            Operator::Project { .. } => {
                // Projection is mostly CPU-bound
                let cpu_cost = input_cardinality * 1.0;
                Ok((cpu_cost, input_cardinality)) // Cardinality unchanged
            }

            Operator::Sort { .. } => {
                // Sort cost using n*log(n) model
                let sort_cost = input_cardinality * (input_cardinality.log2()).max(1.0) * 2.0;
                Ok((sort_cost, input_cardinality))
            }

            Operator::Limit { count, .. } => {
                // Limit reduces both cost and cardinality
                let limit_cost = 1.0;
                let output_cardinality = (*count as f64).min(input_cardinality);
                Ok((limit_cost, output_cardinality))
            }

            Operator::Aggregate {
                source,
                aggregations,
                group_by,
                ..
            } => {
                let group_count = if group_by.is_empty() {
                    1.0 // No grouping
                } else {
                    // Estimate number of groups (simplified)
                    (input_cardinality * 0.1).max(1.0)
                };

                let agg_cost = input_cardinality * 3.0 + group_count * 5.0; // Processing + grouping
                Ok((agg_cost, group_count))
            }

            // Default case for unhandled operators
            _ => {
                let default_cost = input_cardinality * 10.0; // Conservative estimate
                Ok((default_cost, input_cardinality))
            }
        }
    }

    /// Estimate cardinality (number of output rows) for an operator
    fn estimate_operator_cardinality(&self, operators: &[Operator]) -> Result<f64> {
        let mut cardinality = 1.0;
        for operator in operators {
            let (_, output_card) = self.estimate_operator_cost(operator, cardinality)?;
            cardinality = output_card;
        }
        Ok(cardinality)
    }

    /// Estimate filter selectivity based on predicate type
    fn estimate_filter_selectivity(&self, predicate: &str) -> Result<f64> {
        // Simple heuristic based on predicate content
        if predicate.contains('=') && !predicate.contains('!') {
            // Equality filters are selective
            Ok(0.1) // 10% selectivity for equality
        } else if predicate.contains("CONTAINS") || predicate.contains("STARTS WITH") {
            // String matching is moderately selective
            Ok(0.3) // 30% selectivity
        } else if predicate.contains('>') || predicate.contains('<') {
            // Range filters have medium selectivity
            Ok(0.4) // 40% selectivity for ranges
        } else {
            // Default selectivity for complex predicates
            Ok(0.5) // 50% selectivity
        }
    }

    /// Estimate relationship traversal statistics
    fn estimate_relationship_stats(
        &self,
        type_filter: &Option<Vec<u32>>,
    ) -> Result<RelationshipTraversalStats> {
        // For now, return default stats. In production, this would query actual relationship statistics
        Ok(RelationshipTraversalStats {
            total_relationships: 1000,
            total_nodes: 500,
            high_degree_nodes: 10,
            avg_relationships_per_node: 2.0,
            path_cache_hit_rate: 0.8,
            index_hit_rate: 0.9,
        })
    }

    /// Get query plan cache statistics
    pub fn plan_cache_stats(&self) -> &QueryPlanCacheStats {
        self.plan_cache.stats()
    }

    /// Get detailed plan reuse statistics
    pub fn plan_reuse_stats(&self) -> PlanReuseStats {
        self.plan_cache.plan_reuse_stats()
    }

    /// Get aggregation cache statistics
    pub fn aggregation_cache_stats(&self) -> AggregationCacheStats {
        self.aggregation_cache.stats()
    }

    /// Clean expired entries from both caches
    pub fn clean_expired_caches(&mut self) {
        self.plan_cache.clean_expired();
        self.aggregation_cache.clean_expired();
    }

    /// Update performance metrics with query plan cache statistics
    pub async fn update_performance_metrics(
        &self,
        metrics: &crate::performance::PerformanceMetrics,
    ) {
        let stats = self.plan_cache.stats();

        // Update counters
        metrics
            .increment_counter("query_plan_cache_lookups", stats.lookups)
            .await;
        metrics
            .increment_counter("query_plan_cache_hits", stats.hits)
            .await;
        metrics
            .increment_counter("query_plan_cache_misses", stats.misses)
            .await;
        metrics
            .increment_counter("query_plan_cache_evictions", stats.evictions)
            .await;
        metrics
            .increment_counter("query_plan_cache_expirations", stats.expirations)
            .await;

        // Update gauges
        metrics
            .set_gauge(
                "query_plan_cache_hit_rate",
                if stats.lookups > 0 {
                    stats.hits as f64 / stats.lookups as f64
                } else {
                    0.0
                },
            )
            .await;
        metrics
            .set_gauge("query_plan_cache_size", stats.cached_plans as f64)
            .await;
    }

    /// Clear query plan cache
    pub fn clear_plan_cache(&mut self) {
        self.plan_cache.clear();
    }

    /// Clean expired plans from cache
    pub fn clean_expired_plans(&mut self) {
        self.plan_cache.clean_expired();
    }

    /// Optimize operator order based on cost estimates
    pub fn optimize_operator_order(&self, operators: Vec<Operator>) -> Result<Vec<Operator>> {
        if operators.len() <= 1 {
            return Ok(operators);
        }

        // Separate operators into different categories
        let mut scans = Vec::new();
        let mut filters = Vec::new();
        let mut expansions = Vec::new();
        let mut joins = Vec::new();
        let mut others = Vec::new();

        for operator in operators {
            match &operator {
                Operator::NodeByLabel { .. }
                | Operator::AllNodesScan { .. }
                | Operator::IndexScan { .. } => {
                    scans.push(operator);
                }
                Operator::Filter { .. } => {
                    filters.push(operator);
                }
                Operator::Expand { .. } => {
                    expansions.push(operator);
                }
                Operator::Join { .. } => {
                    joins.push(operator);
                }
                _ => {
                    others.push(operator);
                }
            }
        }

        // Optimize scan order: put lowest cost scans first
        let mut scan_costs = Vec::new();
        for (i, scan) in scans.iter().enumerate() {
            let cost = self
                .estimate_plan_cost(std::slice::from_ref(scan))
                .unwrap_or(1000.0);
            scan_costs.push((cost, i));
        }
        scan_costs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut optimized_scans = Vec::new();
        for (_, idx) in scan_costs {
            optimized_scans.push(scans[idx].clone());
        }

        // Optimize join order: put smaller joins first (simple heuristic)
        let mut optimized_joins = Vec::new();
        for join in joins {
            optimized_joins.push(join);
        }

        // Combine in optimal order: scans -> expansions -> filters -> joins -> others
        // Expansions must come before filters because filters may depend on relationship variables
        // created by expansions (e.g., WHERE r.role = 'Developer')
        let mut result = Vec::new();
        result.extend(optimized_scans);
        result.extend(expansions);
        result.extend(filters);
        result.extend(optimized_joins);
        result.extend(others);

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::executor::JoinType;
    use crate::executor::parser::{
        BinaryOperator, Clause, CypherQuery, Expression, LimitClause, Literal, MatchClause,
        NodePattern, Pattern, PatternElement, RelationshipDirection, RelationshipPattern,
        RelationshipQuantifier, ReturnClause, ReturnItem, WhereClause,
    };
    use crate::index::{KnnIndex, LabelIndex};
    use crate::testing::TestContext;

    /// Helper to create a test catalog with guaranteed directory existence
    fn create_test_catalog() -> (Catalog, TestContext) {
        let ctx = TestContext::new();
        let catalog =
            Catalog::with_isolated_path(ctx.path().join("catalog.mdb"), 100 * 1024 * 1024)
                .expect("Failed to create catalog");
        (catalog, ctx)
    }

    #[test]
    fn test_plan_simple_query() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        path_variable: None,
                        elements: vec![PatternElement::Node(NodePattern {
                            variable: Some("n".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                        })],
                    },
                    where_clause: None,
                    optional: false,
                    hints: vec![],
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("n".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert_eq!(operators.len(), 2);

        match &operators[0] {
            Operator::NodeByLabel { variable, .. } => {
                assert_eq!(variable, "n");
            }
            _ => panic!("Expected NodeByLabel operator"),
        }

        match &operators[1] {
            Operator::Project { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].alias, "n");
            }
            _ => panic!("Expected Project operator"),
        }
    }

    #[test]
    fn test_estimate_cost() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let operators = vec![
            Operator::NodeByLabel {
                label_id: 1,
                variable: "n".to_string(),
            },
            Operator::Filter {
                predicate: "n.age > 18".to_string(),
            },
            Operator::Project {
                items: vec![ProjectionItem {
                    alias: "n".to_string(),
                    expression: Expression::Variable("n".to_string()),
                }],
            },
        ];

        let cost = planner.estimate_cost(&operators).unwrap();
        assert!(cost > 0.0);
    }

    #[test]
    fn test_plan_query_with_where_clause() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        path_variable: None,
                        elements: vec![PatternElement::Node(NodePattern {
                            variable: Some("n".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                        })],
                    },
                    where_clause: Some(WhereClause {
                        expression: Expression::BinaryOp {
                            left: Box::new(Expression::PropertyAccess {
                                variable: "n".to_string(),
                                property: "age".to_string(),
                            }),
                            op: BinaryOperator::GreaterThan,
                            right: Box::new(Expression::Literal(Literal::Integer(18))),
                        },
                    }),
                    optional: false,
                    hints: vec![],
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("n".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert_eq!(operators.len(), 3); // NodeByLabel, Filter, Project

        match &operators[0] {
            Operator::NodeByLabel { variable, .. } => {
                assert_eq!(variable, "n");
            }
            _ => panic!("Expected NodeByLabel operator"),
        }

        match &operators[1] {
            Operator::Filter { predicate } => {
                assert!(predicate.contains("n.age"));
                assert!(predicate.contains(">"));
                assert!(predicate.contains("18"));
            }
            _ => panic!("Expected Filter operator"),
        }

        match &operators[2] {
            Operator::Project { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].alias, "n");
            }
            _ => panic!("Expected Project operator"),
        }
    }

    #[test]
    fn test_plan_query_with_limit() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        path_variable: None,
                        elements: vec![PatternElement::Node(NodePattern {
                            variable: Some("n".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                        })],
                    },
                    where_clause: None,
                    optional: false,
                    hints: vec![],
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("n".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
                Clause::Limit(LimitClause {
                    count: Expression::Literal(Literal::Integer(10)),
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert_eq!(operators.len(), 3); // NodeByLabel, Project, Limit

        match &operators[2] {
            Operator::Limit { count } => {
                assert_eq!(*count, 10);
            }
            _ => panic!("Expected Limit operator"),
        }
    }

    #[test]
    fn test_plan_query_with_relationship() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        path_variable: None,
                        elements: vec![
                            PatternElement::Node(NodePattern {
                                variable: Some("a".to_string()),
                                labels: vec!["Person".to_string()],
                                properties: None,
                            }),
                            PatternElement::Relationship(RelationshipPattern {
                                variable: Some("r".to_string()),
                                types: vec!["KNOWS".to_string()],
                                direction: RelationshipDirection::Outgoing,
                                properties: None,
                                quantifier: None,
                            }),
                            PatternElement::Node(NodePattern {
                                variable: Some("b".to_string()),
                                labels: vec!["Person".to_string()],
                                properties: None,
                            }),
                        ],
                    },
                    where_clause: None,
                    optional: false,
                    hints: vec![],
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("a".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert!(operators.len() >= 2); // At least NodeByLabel and Project

        // Check for Expand operator
        let has_expand = operators
            .iter()
            .any(|op| matches!(op, Operator::Expand { .. }));
        assert!(has_expand, "Expected Expand operator for relationship");
    }

    #[test]
    fn test_plan_query_with_variable_length_path() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        path_variable: None,
                        elements: vec![
                            PatternElement::Node(NodePattern {
                                variable: Some("a".to_string()),
                                labels: vec!["Person".to_string()],
                                properties: None,
                            }),
                            PatternElement::Relationship(RelationshipPattern {
                                variable: Some("r".to_string()),
                                types: vec!["KNOWS".to_string()],
                                direction: RelationshipDirection::Outgoing,
                                properties: None,
                                quantifier: Some(RelationshipQuantifier::ZeroOrMore),
                            }),
                            PatternElement::Node(NodePattern {
                                variable: Some("b".to_string()),
                                labels: vec!["Person".to_string()],
                                properties: None,
                            }),
                        ],
                    },
                    where_clause: None,
                    optional: false,
                    hints: vec![],
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("a".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();

        // Check for VariableLengthPath operator
        let has_variable_length_path = operators
            .iter()
            .any(|op| matches!(op, Operator::VariableLengthPath { .. }));
        assert!(
            has_variable_length_path,
            "Expected VariableLengthPath operator for variable-length relationship"
        );

        // Should NOT have regular Expand operator
        let has_expand = operators
            .iter()
            .any(|op| matches!(op, Operator::Expand { .. }));
        assert!(
            !has_expand,
            "Should not have Expand operator when quantifier is present"
        );
    }

    #[test]
    fn test_plan_query_with_range_quantifier() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        path_variable: None,
                        elements: vec![
                            PatternElement::Node(NodePattern {
                                variable: Some("a".to_string()),
                                labels: vec!["Person".to_string()],
                                properties: None,
                            }),
                            PatternElement::Relationship(RelationshipPattern {
                                variable: Some("r".to_string()),
                                types: vec!["KNOWS".to_string()],
                                direction: RelationshipDirection::Outgoing,
                                properties: None,
                                quantifier: Some(RelationshipQuantifier::Range(1, 3)),
                            }),
                            PatternElement::Node(NodePattern {
                                variable: Some("b".to_string()),
                                labels: vec!["Person".to_string()],
                                properties: None,
                            }),
                        ],
                    },
                    where_clause: None,
                    optional: false,
                    hints: vec![],
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("a".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();

        // Check for VariableLengthPath operator with Range quantifier
        let has_variable_length_path = operators.iter().any(|op| {
            if let Operator::VariableLengthPath { quantifier, .. } = op {
                matches!(quantifier, RelationshipQuantifier::Range(1, 3))
            } else {
                false
            }
        });
        assert!(
            has_variable_length_path,
            "Expected VariableLengthPath operator with Range quantifier"
        );
    }

    #[test]
    fn test_plan_query_empty_patterns() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };

        let result = planner.plan_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn test_expression_to_string_variable() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let expr = Expression::Variable("test_var".to_string());
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "test_var");
    }

    #[test]
    fn test_expression_to_string_property_access() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let expr = Expression::PropertyAccess {
            variable: "n".to_string(),
            property: "age".to_string(),
        };
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "n.age");
    }

    #[test]
    fn test_expression_to_string_literals() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        // Test string literal - use single quotes for Neo4j compatibility (fixed in Phase 1)
        let expr = Expression::Literal(Literal::String("hello".to_string()));
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "'hello'");

        // Test integer literal
        let expr = Expression::Literal(Literal::Integer(42));
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "42");

        // Test float literal
        let expr = Expression::Literal(Literal::Float(std::f64::consts::PI));
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "3.141592653589793");

        // Test boolean literal
        let expr = Expression::Literal(Literal::Boolean(true));
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "true");

        // Test null literal
        let expr = Expression::Literal(Literal::Null);
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "NULL");
    }

    #[test]
    fn test_expression_to_string_binary_operators() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let expr = Expression::BinaryOp {
            left: Box::new(Expression::Variable("a".to_string())),
            op: BinaryOperator::Equal,
            right: Box::new(Expression::Variable("b".to_string())),
        };
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "a = b");

        let expr = Expression::BinaryOp {
            left: Box::new(Expression::Variable("x".to_string())),
            op: BinaryOperator::GreaterThan,
            right: Box::new(Expression::Literal(Literal::Integer(10))),
        };
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "x > 10");
    }

    #[test]
    fn test_expression_to_string_parameter() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let expr = Expression::Parameter("param1".to_string());
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "$param1");
    }

    #[test]
    fn test_estimate_cost_all_operators() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let operators = vec![
            Operator::NodeByLabel {
                label_id: 1,
                variable: "n".to_string(),
            },
            Operator::Filter {
                predicate: "n.age > 18".to_string(),
            },
            Operator::Expand {
                type_ids: vec![1],
                source_var: "n".to_string(),
                target_var: "m".to_string(),
                rel_var: "r".to_string(),
                direction: Direction::Outgoing,
            },
            Operator::Project {
                items: vec![ProjectionItem {
                    alias: "n".to_string(),
                    expression: Expression::Variable("n".to_string()),
                }],
            },
            Operator::Limit { count: 10 },
            Operator::Sort {
                columns: vec!["n.name".to_string()],
                ascending: vec![true],
            },
            Operator::Aggregate {
                group_by: vec!["n".to_string()],
                aggregations: vec![],
                projection_items: None,
                source: None,
                streaming_optimized: false,
                push_down_optimized: false,
            },
            Operator::Union {
                left: vec![Operator::NodeByLabel {
                    label_id: 1,
                    variable: "a".to_string(),
                }],
                right: vec![Operator::NodeByLabel {
                    label_id: 2,
                    variable: "b".to_string(),
                }],
                distinct: true,
            },
            Operator::Join {
                left: Box::new(Operator::NodeByLabel {
                    label_id: 1,
                    variable: "a".to_string(),
                }),
                right: Box::new(Operator::NodeByLabel {
                    label_id: 2,
                    variable: "b".to_string(),
                }),
                join_type: JoinType::Inner,
                condition: Some("a.id = b.id".to_string()),
            },
            Operator::IndexScan {
                index_name: "label_Person".to_string(),
                label: "Person".to_string(),
            },
            Operator::Distinct {
                columns: vec!["n".to_string()],
            },
        ];

        let cost = planner.estimate_cost(&operators).unwrap();
        assert!(cost > 0.0);
        // Should be substantial with all operators (adjusted threshold)
        assert!(cost > 100.0);
    }

    #[test]
    fn test_optimize_operator_order() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let operators = vec![
            Operator::NodeByLabel {
                label_id: 1,
                variable: "n".to_string(),
            },
            Operator::Filter {
                predicate: "n.age > 18".to_string(),
            },
        ];

        let optimized = planner.optimize_operator_order(operators.clone()).unwrap();
        assert_eq!(optimized.len(), operators.len());
        // For MVP, should return same order
        // For MVP, should return same order
        assert_eq!(optimized.len(), operators.len());
    }

    #[test]
    fn test_plan_query_with_return_alias() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        path_variable: None,
                        elements: vec![PatternElement::Node(NodePattern {
                            variable: Some("n".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                        })],
                    },
                    where_clause: None,
                    optional: false,
                    hints: vec![],
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("n".to_string()),
                        alias: Some("person".to_string()),
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert_eq!(operators.len(), 2);

        match &operators[1] {
            Operator::Project { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].alias, "person");
            }
            _ => panic!("Expected Project operator with alias"),
        }
    }

    /// Check if aggregation can be optimized with streaming
    pub fn can_use_streaming_aggregation(operators: &[Operator]) -> bool {
        // Check if we have aggregation operations that can benefit from streaming
        for operator in operators {
            if let Operator::Aggregate {
                group_by,
                aggregations,
                ..
            } = operator
            {
                // Streaming is beneficial when:
                // 1. We have aggregations that can be computed incrementally
                // 2. Group-by keys are not too numerous (to avoid memory explosion)
                // 3. We don't have complex expressions in aggregations

                if aggregations.len() > 10 {
                    return false; // Too many aggregations, stick with in-memory
                }

                // Check aggregation types - streaming works best with COUNT, SUM, AVG
                for agg in aggregations {
                    match agg {
                        Aggregation::Count { .. }
                        | Aggregation::Sum { .. }
                        | Aggregation::Avg { .. } => {
                            // These can be streamed
                        }
                        Aggregation::Min { .. } | Aggregation::Max { .. } => {
                            // These can also be streamed
                        }
                        Aggregation::Collect { .. } => {
                            // Collect requires storing all values, not suitable for streaming
                            return false;
                        }
                        Aggregation::CountStarOptimized { .. } => {
                            // Optimized count is already efficient
                        }
                        _ => {
                            // Other aggregations may not be suitable for streaming
                            return false;
                        }
                    }
                }

                // Check group-by complexity
                if group_by.len() > 3 {
                    return false; // Too many group-by keys for streaming
                }

                return true;
            }
        }
        false
    }

    /// Optimize aggregation operations by pushing them down in the query plan
    pub fn optimize_aggregations(operators: Vec<Operator>) -> Result<Vec<Operator>> {
        let mut result = Vec::new();

        for operator in operators {
            match operator {
                Operator::Aggregate {
                    ref aggregations,
                    ref group_by,
                    ref source,
                    ..
                } => {
                    // Check if we can push aggregation down to reduce data volume earlier
                    if let Some(source_op) = source.as_ref() {
                        // Convert group_by from Vec<String> to Vec<Expression> for the check
                        // For now, we'll just check if we can push down (simplified)
                        let can_push = match source_op.as_ref() {
                            Operator::Filter { .. } | Operator::Project { .. } => true,
                            _ => false,
                        };
                        if can_push {
                            // Create a new aggregation operator with push-down optimization
                            let optimized_agg = Operator::Aggregate {
                                aggregations: aggregations.clone(),
                                group_by: group_by.clone(),
                                projection_items: None,
                                source: source.clone(),
                                streaming_optimized: false,
                                push_down_optimized: true,
                            };
                            result.push(optimized_agg);
                            continue;
                        }
                    }

                    // Use streaming aggregation if beneficial
                    if can_use_streaming_aggregation(&[operator.clone()]) {
                        let streaming_agg = Operator::Aggregate {
                            aggregations: aggregations.clone(),
                            group_by: group_by.clone(),
                            projection_items: None,
                            source: source.clone(),
                            streaming_optimized: true,
                            push_down_optimized: false,
                        };
                        result.push(streaming_agg);
                        continue;
                    }

                    // Default aggregation
                    result.push(operator);
                }
                _ => result.push(operator),
            }
        }

        Ok(result)
    }

    /// Check if aggregation can be pushed down to reduce data processing
    fn can_push_aggregation_down(
        source_op: &Operator,
        aggregations: &[Aggregation],
        group_by: &[Expression],
    ) -> bool {
        match source_op {
            Operator::Filter { .. } => {
                // We can push aggregation past filters
                // Filter doesn't have a source field, so we can push down
                return true;
            }
            Operator::Project { .. } => {
                // Check if projection includes all needed columns for aggregation
                // Project doesn't have a source field, so we can push down
                return true;
            }
            Operator::Expand { .. } => {
                // Relationship expansions can sometimes be optimized with aggregation
                // For now, be conservative and don't push down
                return false;
            }
            _ => {
                // Other operators - check if they produce data we need for aggregation
                return source_supports_aggregation(source_op, aggregations, group_by);
            }
        }
    }

    /// Check if a source operator supports aggregation optimization
    fn source_supports_aggregation(
        source_op: &Operator,
        _aggregations: &[Aggregation],
        _group_by: &[Expression],
    ) -> bool {
        match source_op {
            Operator::NodeByLabel { .. }
            | Operator::AllNodesScan { .. }
            | Operator::IndexScan { .. } => {
                // These are good sources for aggregation - they produce nodes we can aggregate
                true
            }
            Operator::Expand { .. } => {
                // Relationship traversal results can be aggregated
                true
            }
            _ => false,
        }
    }

    /// Create optimized COUNT operations
    pub fn optimize_count_operations(operators: Vec<Operator>) -> Result<Vec<Operator>> {
        let mut result = Vec::new();

        for operator in operators {
            match operator {
                Operator::Aggregate {
                    aggregations,
                    group_by,
                    source,
                    ..
                } => {
                    let mut optimized_aggregations = Vec::new();

                    for agg in aggregations {
                        match agg {
                            Aggregation::Count { column: None, .. } => {
                                // Optimize COUNT(*) operations
                                if can_optimize_count_star(&source) {
                                    optimized_aggregations.push(Aggregation::CountStarOptimized {
                                        alias: "count".to_string(), // Default alias
                                    });
                                } else {
                                    optimized_aggregations.push(agg);
                                }
                            }
                            _ => optimized_aggregations.push(agg),
                        }
                    }

                    result.push(Operator::Aggregate {
                        aggregations: optimized_aggregations,
                        group_by,
                        projection_items: None,
                        source,
                        streaming_optimized: false,
                        push_down_optimized: false,
                    });
                }
                _ => result.push(operator),
            }
        }

        Ok(result)
    }

    /// Check if COUNT(*) can be optimized (e.g., using index statistics)
    fn can_optimize_count_star(source: &Option<Box<Operator>>) -> bool {
        if let Some(source_op) = source {
            match source_op.as_ref() {
                Operator::NodeByLabel { label_id, .. } => {
                    // We can potentially use label index statistics for COUNT(*)
                    // This would require label index to track counts per label
                    let _ = label_id; // We'll use this in the future
                    false // For now, not implemented
                }
                Operator::AllNodesScan { .. } => {
                    // For all nodes, we could potentially use total node count
                    false // For now, not implemented
                }
                _ => false,
            }
        } else {
            false
        }
    }
}
