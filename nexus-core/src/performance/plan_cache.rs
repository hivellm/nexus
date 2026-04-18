//! Query plan cache for performance optimization
//!
//! This module provides:
//! - Query plan caching
//! - Cache invalidation on schema changes
//! - LRU eviction policy
//! - Cache statistics

use crate::executor::Operator;
use crate::executor::parser::CypherQuery;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Cached query plan
#[derive(Debug, Clone)]
pub struct CachedPlan {
    /// Original query (normalized)
    pub query_pattern: String,
    /// Parsed AST
    pub ast: CypherQuery,
    /// Compiled operators
    pub operators: Vec<Operator>,
    /// Last access time
    pub last_access: Instant,
    /// Access count
    pub access_count: u64,
    /// Plan size estimate
    pub size_estimate: usize,
}

/// Query plan cache with LRU eviction
pub struct QueryPlanCache {
    /// Cached plans
    plans: Arc<RwLock<HashMap<String, CachedPlan>>>,
    /// Access order for LRU (most recent last)
    access_order: Arc<RwLock<VecDeque<String>>>,
    /// Maximum cache size
    max_size: usize,
    /// Maximum memory usage in bytes (estimated)
    max_memory_bytes: usize,
    /// Current memory usage estimate
    current_memory_bytes: Arc<RwLock<usize>>,
    /// Total cache hits
    hits: Arc<AtomicU64>,
    /// Total cache misses
    misses: Arc<AtomicU64>,
}

impl QueryPlanCache {
    /// Create a new query plan cache
    pub fn new(max_size: usize, max_memory_mb: usize) -> Self {
        Self {
            plans: Arc::new(RwLock::new(HashMap::new())),
            access_order: Arc::new(RwLock::new(VecDeque::new())),
            max_size,
            max_memory_bytes: max_memory_mb * 1024 * 1024,
            current_memory_bytes: Arc::new(RwLock::new(0)),
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get a cached plan
    pub fn get(&self, query_pattern: &str) -> Option<CachedPlan> {
        let mut plans = self.plans.write().unwrap();
        let mut access_order = self.access_order.write().unwrap();

        if let Some(plan) = plans.get_mut(query_pattern) {
            // Update access time and count
            plan.last_access = Instant::now();
            plan.access_count += 1;

            // Move to end of access order (most recent)
            if let Some(pos) = access_order.iter().position(|q| q == query_pattern) {
                access_order.remove(pos);
            }
            access_order.push_back(query_pattern.to_string());

            // Record cache hit
            self.hits.fetch_add(1, Ordering::SeqCst);

            Some(plan.clone())
        } else {
            // Record cache miss
            self.misses.fetch_add(1, Ordering::SeqCst);
            None
        }
    }

    /// Check if a query pattern exists in cache (without updating access)
    /// Returns (exists, was_hit) - useful for tracking per-query cache status
    pub fn check_cache_status(&self, query_pattern: &str) -> (bool, bool) {
        let plans = self.plans.read().unwrap();
        let exists = plans.contains_key(query_pattern);
        (exists, exists)
    }

    /// Get cache metrics for a specific query pattern
    /// Returns (hits, misses) for tracking per-query metrics
    pub fn get_query_cache_metrics(&self, query_pattern: &str) -> (u64, u64) {
        let plans = self.plans.read().unwrap();
        if let Some(plan) = plans.get(query_pattern) {
            // Return access count as hits (simplified - in reality we'd track separately)
            (plan.access_count, 0)
        } else {
            (0, 1) // Query not in cache = miss
        }
    }

    /// Store a plan in cache
    pub fn put(&self, query_pattern: String, ast: CypherQuery, operators: Vec<Operator>) {
        let size_estimate = self.estimate_plan_size(&ast, &operators);
        let plan = CachedPlan {
            query_pattern: query_pattern.clone(),
            ast,
            operators,
            last_access: Instant::now(),
            access_count: 1,
            size_estimate,
        };

        let mut plans = self.plans.write().unwrap();
        let mut access_order = self.access_order.write().unwrap();
        let mut current_memory = self.current_memory_bytes.write().unwrap();

        // Check if we need to evict
        while plans.len() >= self.max_size
            || *current_memory + size_estimate > self.max_memory_bytes
        {
            if let Some(oldest_pattern) = access_order.pop_front() {
                if let Some(old_plan) = plans.remove(&oldest_pattern) {
                    *current_memory = current_memory.saturating_sub(old_plan.size_estimate);
                }
            } else {
                break;
            }
        }

        // Insert new plan
        if let Some(old_plan) = plans.insert(query_pattern.clone(), plan) {
            *current_memory = current_memory.saturating_sub(old_plan.size_estimate);
        }
        *current_memory += size_estimate;
        access_order.push_back(query_pattern);
    }

    /// Estimate plan size in bytes
    fn estimate_plan_size(&self, _ast: &CypherQuery, operators: &[Operator]) -> usize {
        // Rough estimate: each operator ~100 bytes, AST ~500 bytes
        operators.len() * 100 + 500
    }

    /// Invalidate cache (clear all entries)
    pub fn invalidate_all(&self) {
        self.plans.write().unwrap().clear();
        self.access_order.write().unwrap().clear();
        *self.current_memory_bytes.write().unwrap() = 0;
        // Note: We don't reset hits/misses here as they represent historical statistics
    }

    /// Invalidate plans matching a pattern (e.g., when schema changes)
    pub fn invalidate_pattern(&self, pattern: &str) {
        let mut plans = self.plans.write().unwrap();
        let mut access_order = self.access_order.write().unwrap();
        let mut current_memory = self.current_memory_bytes.write().unwrap();

        let keys_to_remove: Vec<String> = plans
            .keys()
            .filter(|k| k.contains(pattern))
            .cloned()
            .collect();

        for key in keys_to_remove {
            if let Some(plan) = plans.remove(&key) {
                *current_memory = current_memory.saturating_sub(plan.size_estimate);
                access_order.retain(|k| k != &key);
            }
        }
    }

    /// Get cache statistics
    pub fn get_statistics(&self) -> PlanCacheStatistics {
        let plans = self.plans.read().unwrap();
        let _access_order = self.access_order.read().unwrap();
        let current_memory = self.current_memory_bytes.read().unwrap();

        let hits = self.hits.load(Ordering::SeqCst);
        let misses = self.misses.load(Ordering::SeqCst);
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        PlanCacheStatistics {
            cached_plans: plans.len(),
            max_size: self.max_size,
            current_memory_bytes: *current_memory,
            max_memory_bytes: self.max_memory_bytes,
            hit_rate,
            hits,
            misses,
        }
    }

    /// Get current cache hits count
    pub fn get_hits(&self) -> u64 {
        self.hits.load(Ordering::SeqCst)
    }

    /// Get current cache misses count
    pub fn get_misses(&self) -> u64 {
        self.misses.load(Ordering::SeqCst)
    }

    /// Reset cache statistics
    pub fn reset_stats(&self) {
        self.hits.store(0, Ordering::SeqCst);
        self.misses.store(0, Ordering::SeqCst);
    }

    /// Clear old entries based on age
    pub fn evict_old(&self, max_age: Duration) {
        let now = Instant::now();
        let mut plans = self.plans.write().unwrap();
        let mut access_order = self.access_order.write().unwrap();
        let mut current_memory = self.current_memory_bytes.write().unwrap();

        let keys_to_remove: Vec<String> = plans
            .iter()
            .filter(|(_, plan)| now.duration_since(plan.last_access) > max_age)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            if let Some(plan) = plans.remove(&key) {
                *current_memory = current_memory.saturating_sub(plan.size_estimate);
                access_order.retain(|k| k != &key);
            }
        }
    }
}

/// Plan cache statistics
#[derive(Debug, Clone)]
pub struct PlanCacheStatistics {
    /// Number of cached plans
    pub cached_plans: usize,
    /// Maximum cache size
    pub max_size: usize,
    /// Current memory usage in bytes
    pub current_memory_bytes: usize,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: usize,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
    /// Total cache hits
    pub hits: u64,
    /// Total cache misses
    pub misses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_cache() {
        let cache = QueryPlanCache::new(10, 10); // 10 plans, 10MB

        // Create a dummy AST and operators
        let ast = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };
        let operators = vec![];

        cache.put(
            "MATCH (n) RETURN n".to_string(),
            ast.clone(),
            operators.clone(),
        );

        assert!(cache.get("MATCH (n) RETURN n").is_some());
        assert!(cache.get("MATCH (m) RETURN m").is_none());
    }

    #[test]
    fn test_cache_eviction() {
        let cache = QueryPlanCache::new(2, 10); // Small cache for testing

        let ast = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };
        let operators = vec![];

        cache.put("query1".to_string(), ast.clone(), operators.clone());
        cache.put("query2".to_string(), ast.clone(), operators.clone());
        cache.put("query3".to_string(), ast.clone(), operators.clone()); // Should evict query1

        assert!(cache.get("query1").is_none()); // Evicted
        assert!(cache.get("query2").is_some());
        assert!(cache.get("query3").is_some());
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = QueryPlanCache::new(10, 10);

        let ast = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };
        let operators = vec![];

        cache.put(
            "MATCH (n:Person) RETURN n".to_string(),
            ast.clone(),
            operators.clone(),
        );
        cache.put(
            "MATCH (n:Company) RETURN n".to_string(),
            ast.clone(),
            operators.clone(),
        );

        cache.invalidate_all();

        assert!(cache.get("MATCH (n:Person) RETURN n").is_none());
        assert!(cache.get("MATCH (n:Company) RETURN n").is_none());
    }

    #[test]
    fn test_cache_statistics() {
        let cache = QueryPlanCache::new(10, 10);

        let ast = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };
        let operators = vec![];

        cache.put("QUERY1".to_string(), ast.clone(), operators.clone());
        cache.put("QUERY2".to_string(), ast.clone(), operators.clone());

        let stats = cache.get_statistics();
        assert_eq!(stats.cached_plans, 2);
        assert_eq!(stats.max_size, 10);
        assert!(stats.current_memory_bytes > 0);
    }

    #[test]
    fn test_cache_access_count() {
        let cache = QueryPlanCache::new(10, 10);

        let ast = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };
        let operators = vec![];

        cache.put("QUERY1".to_string(), ast.clone(), operators.clone());

        // Access multiple times
        let plan1 = cache.get("QUERY1");
        assert!(plan1.is_some());
        let plan2 = cache.get("QUERY1");
        assert!(plan2.is_some());

        // Access count should increase
        let plan3 = cache.get("QUERY1");
        assert!(plan3.is_some());
        assert!(plan3.unwrap().access_count >= 3);
    }

    #[test]
    fn test_cache_invalidate_pattern() {
        let cache = QueryPlanCache::new(10, 10);

        let ast = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };
        let operators = vec![];

        cache.put(
            "MATCH (n:Person) RETURN n".to_string(),
            ast.clone(),
            operators.clone(),
        );
        cache.put(
            "MATCH (n:Company) RETURN n".to_string(),
            ast.clone(),
            operators.clone(),
        );
        cache.put(
            "CREATE (n:Person)".to_string(),
            ast.clone(),
            operators.clone(),
        );

        // Invalidate Person-related queries
        cache.invalidate_pattern("Person");

        assert!(cache.get("MATCH (n:Person) RETURN n").is_none());
        assert!(cache.get("CREATE (n:Person)").is_none());
        assert!(cache.get("MATCH (n:Company) RETURN n").is_some()); // Should still exist
    }

    #[test]
    fn test_cache_evict_old() {
        let cache = QueryPlanCache::new(10, 10);

        let ast = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };
        let operators = vec![];

        cache.put("QUERY1".to_string(), ast.clone(), operators.clone());

        // Wait a bit and evict old entries
        std::thread::sleep(std::time::Duration::from_millis(10));
        cache.evict_old(Duration::from_millis(5));

        // Query should be evicted
        assert!(cache.get("QUERY1").is_none());
    }

    #[test]
    fn test_cache_lru_order() {
        let cache = QueryPlanCache::new(3, 10);

        let ast = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };
        let operators = vec![];

        cache.put("query1".to_string(), ast.clone(), operators.clone());
        cache.put("query2".to_string(), ast.clone(), operators.clone());
        cache.put("query3".to_string(), ast.clone(), operators.clone());

        // Access query1 to make it most recent
        cache.get("query1");

        // Add query4 - should evict query2 (least recently used)
        cache.put("query4".to_string(), ast.clone(), operators.clone());

        assert!(cache.get("query1").is_some()); // Most recent
        assert!(cache.get("query2").is_none()); // Evicted
        assert!(cache.get("query3").is_some());
        assert!(cache.get("query4").is_some());
    }
}
