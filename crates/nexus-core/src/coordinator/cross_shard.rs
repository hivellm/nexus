//! Cross-shard traversal primitives.
//!
//! When a Cypher expand crosses a shard boundary — source on shard A,
//! destination on shard B — the coordinator fetches the destination's
//! labels / properties from B via [`RemoteNodeFetcher::fetch`]. This
//! module owns:
//!
//! * [`RemoteNodeView`] — the value type returned by a remote fetch.
//! * [`CrossShardCache`] — a TTL+generation-aware LRU in front of the
//!   fetcher, so the hot remote nodes in a traversal don't generate
//!   one RPC per step.
//! * [`FetchBudget`] — per-query bound on remote RPCs, enforced by the
//!   coordinator. Variable-length traversals that explode the fan-out
//!   surface `ERR_TOO_MANY_REMOTE_FETCHES` rather than eating all the
//!   cluster's cycles.
//! * [`RemoteNodeFetcher`] trait — the narrow RPC surface the
//!   coordinator calls; production plugs a TCP impl, tests use
//!   [`InMemoryFetcher`].

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sharding::metadata::ShardId;
use thiserror::Error;

/// The internal-form node id used by the storage layer.
pub type StorageNodeId = u64;

/// View of a node fetched from a remote shard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteNodeView {
    /// Shard that owns the node.
    pub shard_id: ShardId,
    /// Storage-layer node id.
    pub node_id: StorageNodeId,
    /// Label bitmap (same shape as a local node record).
    pub label_bits: u64,
    /// Pointer to the first relationship, for reverse-direction
    /// expands.
    pub first_rel_ptr: u64,
    /// Node properties as decoded JSON.
    pub properties: serde_json::Map<String, Value>,
    /// Generation this view was sampled at.
    pub generation: u64,
}

/// Errors surfaced by cross-shard traversal.
#[derive(Debug, Error)]
pub enum CrossShardError {
    /// The caller exceeded [`FetchBudget::max`].
    #[error("ERR_TOO_MANY_REMOTE_FETCHES: {performed} fetches exceeds budget {budget}")]
    TooManyFetches { performed: u32, budget: u32 },
    /// RPC-level failure. Bubbled up to the coordinator which maps to
    /// `ERR_SHARD_FAILURE`.
    #[error("remote-node fetch failed for shard={shard}, node={node}: {reason}")]
    FetchFailed {
        shard: ShardId,
        node: StorageNodeId,
        reason: String,
    },
}

/// A budget over remote-node fetches for one query. Shared across the
/// query's operators via [`FetchBudget::checked_increment`].
#[derive(Debug)]
pub struct FetchBudget {
    max: u32,
    used: Mutex<u32>,
}

impl FetchBudget {
    /// Fresh budget of `max` fetches.
    #[must_use]
    pub fn new(max: u32) -> Self {
        Self {
            max,
            used: Mutex::new(0),
        }
    }

    /// Bump the counter. Returns `Err` when the budget is exhausted.
    pub fn checked_increment(&self) -> Result<u32, CrossShardError> {
        let mut used = self.used.lock().expect("FetchBudget mutex poisoned");
        if *used >= self.max {
            return Err(CrossShardError::TooManyFetches {
                performed: *used,
                budget: self.max,
            });
        }
        *used += 1;
        Ok(*used)
    }

    /// Current usage.
    #[must_use]
    pub fn used(&self) -> u32 {
        *self.used.lock().expect("FetchBudget mutex poisoned")
    }

    /// Capacity.
    #[inline]
    #[must_use]
    pub fn max(&self) -> u32 {
        self.max
    }
}

/// A fetcher knows how to resolve `(shard_id, node_id)` into a view.
pub trait RemoteNodeFetcher: Send + Sync {
    /// Fetch `node_id` from `shard`. Implementations may fail — the
    /// error is forwarded through the coordinator.
    fn fetch(&self, shard: ShardId, node: StorageNodeId)
    -> Result<RemoteNodeView, CrossShardError>;
}

/// TTL + generation-aware LRU cache for remote views.
#[derive(Debug)]
pub struct CrossShardCache {
    capacity: usize,
    ttl: Duration,
    inner: Mutex<CacheInner>,
}

#[derive(Debug)]
struct CacheInner {
    /// Keyed by `(shard_id, node_id, generation)`.
    entries: HashMap<CacheKey, CacheEntry>,
    /// LRU queue — front is most-recently used.
    queue: Vec<CacheKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    shard: ShardId,
    node: StorageNodeId,
    generation: u64,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    view: RemoteNodeView,
    inserted_at: Instant,
}

impl CrossShardCache {
    /// Build a cache with `capacity` entries and `ttl` safety net.
    #[must_use]
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            capacity,
            ttl,
            inner: Mutex::new(CacheInner {
                entries: HashMap::with_capacity(capacity.max(1)),
                queue: Vec::with_capacity(capacity.max(1)),
            }),
        }
    }

    /// Lookup a view. Returns `None` on miss, expired TTL, or
    /// generation mismatch.
    #[must_use]
    pub fn get(
        &self,
        shard: ShardId,
        node: StorageNodeId,
        generation: u64,
    ) -> Option<RemoteNodeView> {
        let key = CacheKey {
            shard,
            node,
            generation,
        };
        let mut inner = self.inner.lock().expect("CrossShardCache mutex poisoned");
        let entry = inner.entries.get(&key).cloned()?;
        if entry.inserted_at.elapsed() >= self.ttl {
            inner.entries.remove(&key);
            inner.queue.retain(|k| k != &key);
            return None;
        }
        // Touch: move to front of LRU.
        if let Some(pos) = inner.queue.iter().position(|k| k == &key) {
            let k = inner.queue.remove(pos);
            inner.queue.insert(0, k);
        }
        Some(entry.view)
    }

    /// Insert a freshly-fetched view. Evicts the least-recently-used
    /// entry when over capacity.
    pub fn insert(&self, view: RemoteNodeView) {
        let key = CacheKey {
            shard: view.shard_id,
            node: view.node_id,
            generation: view.generation,
        };
        let mut inner = self.inner.lock().expect("CrossShardCache mutex poisoned");
        if inner.entries.contains_key(&key) {
            inner.entries.insert(
                key.clone(),
                CacheEntry {
                    view,
                    inserted_at: Instant::now(),
                },
            );
            return;
        }
        if inner.entries.len() >= self.capacity {
            if let Some(victim) = inner.queue.pop() {
                inner.entries.remove(&victim);
            }
        }
        inner.entries.insert(
            key.clone(),
            CacheEntry {
                view,
                inserted_at: Instant::now(),
            },
        );
        inner.queue.insert(0, key);
    }

    /// Number of entries currently in the cache.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("CrossShardCache mutex poisoned")
            .entries
            .len()
    }

    /// Flush every cached entry. Used when the coordinator observes a
    /// generation bump it's sure will invalidate everything.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().expect("CrossShardCache mutex poisoned");
        inner.entries.clear();
        inner.queue.clear();
    }
}

/// In-memory fetcher for tests. Routes `(shard, node)` → fixed view.
/// Also counts per-key fetches so tests can assert cache hit rates.
pub struct InMemoryFetcher {
    views: Mutex<HashMap<(ShardId, StorageNodeId), RemoteNodeView>>,
    calls: Mutex<HashMap<(ShardId, StorageNodeId), u32>>,
}

impl InMemoryFetcher {
    /// Empty fetcher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            views: Mutex::new(HashMap::new()),
            calls: Mutex::new(HashMap::new()),
        }
    }

    /// Register `view` as the response to `fetch(shard, node)`.
    pub fn insert(&self, view: RemoteNodeView) {
        let mut v = self.views.lock().expect("InMemoryFetcher mutex poisoned");
        v.insert((view.shard_id, view.node_id), view);
    }

    /// Times `(shard, node)` has been fetched through this client.
    #[must_use]
    pub fn call_count(&self, shard: ShardId, node: StorageNodeId) -> u32 {
        let c = self.calls.lock().expect("InMemoryFetcher mutex poisoned");
        c.get(&(shard, node)).copied().unwrap_or(0)
    }
}

impl Default for InMemoryFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteNodeFetcher for InMemoryFetcher {
    fn fetch(
        &self,
        shard: ShardId,
        node: StorageNodeId,
    ) -> Result<RemoteNodeView, CrossShardError> {
        {
            let mut c = self.calls.lock().expect("InMemoryFetcher mutex poisoned");
            *c.entry((shard, node)).or_insert(0) += 1;
        }
        let v = self.views.lock().expect("InMemoryFetcher mutex poisoned");
        v.get(&(shard, node))
            .cloned()
            .ok_or_else(|| CrossShardError::FetchFailed {
                shard,
                node,
                reason: "not registered".into(),
            })
    }
}

/// High-level helper: fetch from cache, else fall back to the
/// underlying fetcher + insert into cache + bump budget.
pub fn fetch_cached(
    cache: &CrossShardCache,
    fetcher: &dyn RemoteNodeFetcher,
    budget: &FetchBudget,
    shard: ShardId,
    node: StorageNodeId,
    generation: u64,
) -> Result<RemoteNodeView, CrossShardError> {
    if let Some(view) = cache.get(shard, node, generation) {
        return Ok(view);
    }
    budget.checked_increment()?;
    let view = fetcher.fetch(shard, node)?;
    cache.insert(view.clone());
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(shard: u32, node: StorageNodeId, gen_: u64) -> RemoteNodeView {
        RemoteNodeView {
            shard_id: ShardId::new(shard),
            node_id: node,
            label_bits: 0b11,
            first_rel_ptr: 999,
            properties: {
                let mut m = serde_json::Map::new();
                m.insert("name".into(), Value::from(format!("n{node}")));
                m
            },
            generation: gen_,
        }
    }

    #[test]
    fn budget_increments_until_exhausted() {
        let b = FetchBudget::new(3);
        assert_eq!(b.checked_increment().unwrap(), 1);
        assert_eq!(b.checked_increment().unwrap(), 2);
        assert_eq!(b.checked_increment().unwrap(), 3);
        let err = b.checked_increment().unwrap_err();
        assert!(matches!(err, CrossShardError::TooManyFetches { .. }));
        assert_eq!(b.used(), 3);
    }

    #[test]
    fn cache_miss_on_cold_key() {
        let c = CrossShardCache::new(10, Duration::from_secs(30));
        assert!(c.get(ShardId::new(0), 1, 1).is_none());
    }

    #[test]
    fn cache_hit_after_insert() {
        let c = CrossShardCache::new(10, Duration::from_secs(30));
        c.insert(view(1, 42, 5));
        let got = c.get(ShardId::new(1), 42, 5).unwrap();
        assert_eq!(got.node_id, 42);
    }

    #[test]
    fn cache_generation_mismatch_misses() {
        let c = CrossShardCache::new(10, Duration::from_secs(30));
        c.insert(view(1, 42, 5));
        assert!(c.get(ShardId::new(1), 42, 6).is_none());
    }

    #[test]
    fn cache_ttl_expires_entries() {
        let c = CrossShardCache::new(10, Duration::from_millis(10));
        c.insert(view(1, 42, 5));
        std::thread::sleep(Duration::from_millis(20));
        assert!(c.get(ShardId::new(1), 42, 5).is_none());
    }

    #[test]
    fn cache_evicts_lru_on_overflow() {
        let c = CrossShardCache::new(2, Duration::from_secs(30));
        c.insert(view(0, 1, 1));
        c.insert(view(0, 2, 1));
        // Touch 1 so 2 becomes the LRU victim.
        let _ = c.get(ShardId::new(0), 1, 1);
        c.insert(view(0, 3, 1));
        assert!(c.get(ShardId::new(0), 2, 1).is_none(), "expected 2 evicted");
        assert!(c.get(ShardId::new(0), 1, 1).is_some());
        assert!(c.get(ShardId::new(0), 3, 1).is_some());
    }

    #[test]
    fn cache_clear_empties() {
        let c = CrossShardCache::new(10, Duration::from_secs(30));
        c.insert(view(0, 1, 1));
        c.insert(view(0, 2, 1));
        c.clear();
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn fetch_cached_uses_cache_on_second_call() {
        let cache = CrossShardCache::new(10, Duration::from_secs(30));
        let fetcher = InMemoryFetcher::new();
        fetcher.insert(view(1, 42, 5));
        let budget = FetchBudget::new(10);

        let _ = fetch_cached(&cache, &fetcher, &budget, ShardId::new(1), 42, 5).unwrap();
        let _ = fetch_cached(&cache, &fetcher, &budget, ShardId::new(1), 42, 5).unwrap();

        assert_eq!(fetcher.call_count(ShardId::new(1), 42), 1);
        assert_eq!(budget.used(), 1);
    }

    #[test]
    fn fetch_cached_refetches_on_generation_bump() {
        let cache = CrossShardCache::new(10, Duration::from_secs(30));
        let fetcher = InMemoryFetcher::new();
        let budget = FetchBudget::new(10);

        fetcher.insert(view(1, 42, 5));
        fetch_cached(&cache, &fetcher, &budget, ShardId::new(1), 42, 5).unwrap();

        fetcher.insert(view(1, 42, 6));
        fetch_cached(&cache, &fetcher, &budget, ShardId::new(1), 42, 6).unwrap();

        assert_eq!(fetcher.call_count(ShardId::new(1), 42), 2);
        assert_eq!(budget.used(), 2);
    }

    #[test]
    fn fetch_cached_surfaces_fetcher_error() {
        let cache = CrossShardCache::new(10, Duration::from_secs(30));
        let fetcher = InMemoryFetcher::new();
        let budget = FetchBudget::new(10);

        let err = fetch_cached(&cache, &fetcher, &budget, ShardId::new(1), 42, 5).unwrap_err();
        assert!(matches!(err, CrossShardError::FetchFailed { .. }));
        // Budget was debited — upstream could choose to roll it back,
        // but the contract says fetches count regardless of outcome.
        assert_eq!(budget.used(), 1);
    }

    #[test]
    fn fetch_cached_respects_budget() {
        let cache = CrossShardCache::new(10, Duration::from_secs(30));
        let fetcher = InMemoryFetcher::new();
        for n in 0..3 {
            fetcher.insert(view(0, n, 1));
        }
        let budget = FetchBudget::new(2);

        fetch_cached(&cache, &fetcher, &budget, ShardId::new(0), 0, 1).unwrap();
        fetch_cached(&cache, &fetcher, &budget, ShardId::new(0), 1, 1).unwrap();
        let err = fetch_cached(&cache, &fetcher, &budget, ShardId::new(0), 2, 1).unwrap_err();
        assert!(matches!(err, CrossShardError::TooManyFetches { .. }));
    }

    #[test]
    fn cache_update_refreshes_ttl() {
        let c = CrossShardCache::new(10, Duration::from_millis(50));
        c.insert(view(1, 42, 5));
        std::thread::sleep(Duration::from_millis(20));
        // Re-insert same key — should refresh inserted_at.
        c.insert(view(1, 42, 5));
        std::thread::sleep(Duration::from_millis(20));
        // 40ms total but we reset at 20ms, so ~20ms < 50ms TTL. Should
        // still be fresh.
        assert!(c.get(ShardId::new(1), 42, 5).is_some());
    }
}
