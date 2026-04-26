//! Query planner façade.
//!
//! - `queries` — the bulk of `impl QueryPlanner` (cost-based optimisation,
//!   pattern reordering, join algorithm choice, index push-down).
//! - `tests` — cfg(test) harness.

pub mod preparse;
pub mod queries;

#[cfg(test)]
mod tests;

pub use preparse::{PlanHint, extract_plan_hints};

use super::parser::{
    BinaryOperator, Clause, CypherQuery, Expression, Literal, NodePattern, Pattern, PatternElement,
    PropertyMap, QuantifiedGroup, QueryHint, RelationshipDirection, RelationshipPattern,
    RelationshipQuantifier, ReturnItem, SortDirection, UnaryOperator,
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
