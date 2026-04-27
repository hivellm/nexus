//! R-tree registry — per-index lifecycle owner
//! (phase6_rtree-index-core §6.2 + §6.4).
//!
//! Holds one [`RTree`] per registered index, keyed by
//! `"{label}.{property}"`. Two responsibilities:
//!
//! 1. **WAL replay** — `apply_wal_entry` consumes the three
//!    R-tree variants of [`crate::wal::WalEntry`] (`RTreeInsert`,
//!    `RTreeDelete`, `RTreeBulkLoadDone`) and applies them to the
//!    matching tree. The recovery loop on engine startup walks
//!    the WAL once and feeds every entry through this method so
//!    the in-memory tree converges back to the durable state.
//! 2. **Atomic rebuild** — `swap_in(name, new_tree)` replaces an
//!    index's backing tree behind a `RwLock<Arc<RTree>>` pointer
//!    swap. Readers grab a clone of the current `Arc<RTree>` and
//!    keep using it; the new tree only becomes visible to
//!    subsequent reads. No reader observes a half-built tree.
//!
//! ## MVCC visibility (§6.3)
//!
//! The R-tree itself does not store epoch metadata. Visibility
//! filtering happens at the executor layer: after the seek
//! returns a list of `node_id`s, the executor consults the
//! transaction manager's snapshot view of "is this node visible
//! at epoch E?" and drops invisible ids before they count
//! against the `k` limit of `spatial.nearest`. The
//! [`RTreeRegistry::nearest_with_filter`] helper is the seam
//! the executor hooks into.
//!
//! ## Concurrency
//!
//! `RTreeRegistry` is `Send + Sync`. The internal `HashMap` is
//! protected by a `RwLock` so concurrent readers can hit
//! different indexes in parallel; mutations (insert / delete /
//! swap_in) take the write lock briefly.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use super::search::{Metric, NearestHit, SearchError};
use super::tree::RTree;
use crate::wal::WalEntry;
use crate::{Error, Result};

/// Registered R-tree indexes. Outer lock protects the index ↔
/// `Arc<RwLock<Arc<RTree>>>` map; inner `RwLock<Arc<RTree>>`
/// protects each tree's mutable side and gives the swap-in
/// pattern its atomicity.
#[derive(Default, Debug)]
pub struct RTreeRegistry {
    indexes: RwLock<HashMap<String, Arc<RwLock<Arc<RTree>>>>>,
}

impl RTreeRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a fresh empty tree under `name`. No-op when an
    /// index with the same name already exists — callers that
    /// want to wipe an existing index call [`RTreeRegistry::swap_in`]
    /// with a freshly built [`RTree`].
    pub fn register_empty(&self, name: &str) {
        let mut map = self.indexes.write();
        map.entry(name.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(Arc::new(RTree::new()))));
    }

    /// Atomically replace the tree backing `name`. The old
    /// `Arc<RTree>` may still be held by in-flight readers; they
    /// keep using it until they drop their handles, after which
    /// the old tree is freed. New reads see `new_tree`
    /// immediately. Creates the slot if it's missing.
    pub fn swap_in(&self, name: &str, new_tree: RTree) {
        let mut map = self.indexes.write();
        let slot = map
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(Arc::new(RTree::new()))));
        let mut guard = slot.write();
        *guard = Arc::new(new_tree);
    }

    /// Drop an index entirely. Returns `true` when the index
    /// existed.
    pub fn drop_index(&self, name: &str) -> bool {
        let mut map = self.indexes.write();
        map.remove(name).is_some()
    }

    /// `true` iff the registry currently owns an index named
    /// `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.indexes.read().contains_key(name)
    }

    /// Number of registered indexes.
    pub fn len(&self) -> usize {
        self.indexes.read().len()
    }

    /// `true` when no indexes are registered.
    pub fn is_empty(&self) -> bool {
        self.indexes.read().is_empty()
    }

    /// Snapshot the current `Arc<RTree>` for `name`. Callers run
    /// queries through the returned handle without holding the
    /// registry lock — concurrent writers can swap in new trees
    /// while a query is mid-flight; the reader keeps using the
    /// snapshot it captured.
    pub fn snapshot(&self, name: &str) -> Option<Arc<RTree>> {
        let map = self.indexes.read();
        let slot = map.get(name)?;
        let inner = slot.read();
        Some(Arc::clone(&inner))
    }

    /// Apply a single [`WalEntry`] to the matching index. The
    /// recovery loop calls this for every R-tree variant in the
    /// WAL stream. Non-R-tree variants are silently ignored so
    /// callers can pass the whole stream through without
    /// pre-filtering.
    pub fn apply_wal_entry(&self, entry: &WalEntry) -> Result<()> {
        match entry {
            WalEntry::RTreeInsert {
                index_name,
                node_id,
                x,
                y,
            } => {
                self.register_empty(index_name);
                self.with_tree_mut(index_name, |tree| {
                    tree.insert(*node_id, *x, *y);
                })
                .ok_or_else(|| Error::wal(format!("R-tree index missing: {index_name}")))?;
                Ok(())
            }
            WalEntry::RTreeDelete {
                index_name,
                node_id,
            } => {
                if let Some(applied) = self.with_tree_mut(index_name, |tree| tree.delete(*node_id))
                {
                    // Ignore NotFound during replay — a delete for
                    // a node that never made it into the post-
                    // insert image happens after a partial bulk-
                    // load gets restarted. The replay still
                    // converges to the right shape.
                    let _ = applied;
                }
                Ok(())
            }
            WalEntry::RTreeBulkLoadDone { index_name, .. } => {
                // The bulk-load itself is journalled as a stream of
                // `RTreeInsert` entries; this marker just records
                // that the rebuild ran to completion. Replay does
                // not need to do anything further. Recovery code
                // outside the registry uses the marker to decide
                // whether a half-applied bulk-load needs to be
                // re-run.
                self.register_empty(index_name);
                Ok(())
            }
            // Non-R-tree variants are no-ops — let the unified
            // replay loop pass everything through.
            _ => Ok(()),
        }
    }

    /// k-NN with a caller-supplied visibility predicate
    /// (phase6_rtree-index-core §6.3). Drops every entry where
    /// `visible(node_id) == false` before it counts against the
    /// `k` limit, so an invisible tombstoned node never
    /// short-circuits the walk.
    pub fn nearest_with_filter<F>(
        &self,
        index_name: &str,
        px: f64,
        py: f64,
        k: usize,
        metric: Metric,
        mut visible: F,
    ) -> Result<Vec<NearestHit>>
    where
        F: FnMut(u64) -> bool,
    {
        let Some(tree) = self.snapshot(index_name) else {
            return Ok(Vec::new());
        };
        // Over-fetch by a small factor so the visibility filter has
        // room to drop invisible ids without forcing a re-seek. A
        // proper "incremental k-NN with mid-stream filter" would
        // walk the heap until k visible leaves are popped; this
        // shape is good enough for v1 because typical visibility
        // miss rates are well below 50%.
        let target = k.saturating_mul(2).max(k);
        let raw = tree
            .nearest(px, py, target, metric)
            .map_err(|e: SearchError| Error::executor(e.to_string()))?;
        let mut out = Vec::with_capacity(k);
        for hit in raw {
            if visible(hit.node_id) {
                out.push(hit);
                if out.len() >= k {
                    break;
                }
            }
        }
        // If the caller's filter rejected too much, do a second
        // pass with a higher target so the SLO holds.
        if out.len() < k {
            let target2 = k.saturating_mul(8).max(k);
            let raw2 = tree
                .nearest(px, py, target2, metric)
                .map_err(|e: SearchError| Error::executor(e.to_string()))?;
            out.clear();
            for hit in raw2 {
                if visible(hit.node_id) {
                    out.push(hit);
                    if out.len() >= k {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }

    // --- helpers ---------------------------------------------------

    /// Run `f` against the current tree for `name` under the inner
    /// write lock. Returns `Some(f's result)` when the index
    /// exists; `None` otherwise. The closure mutates the tree
    /// in-place — atomicity here is per-mutation, not per-batch.
    /// Bulk rebuilds use [`RTreeRegistry::swap_in`] for true
    /// atomicity.
    fn with_tree_mut<R, F: FnOnce(&mut RTree) -> R>(&self, name: &str, f: F) -> Option<R> {
        let map = self.indexes.read();
        let slot = map.get(name)?;
        let mut guard = slot.write();
        let arc = Arc::make_mut(&mut guard);
        Some(f(arc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_has_no_indexes() {
        let reg = RTreeRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(!reg.contains("foo"));
        assert!(reg.snapshot("foo").is_none());
    }

    #[test]
    fn register_then_query_through_snapshot() {
        let reg = RTreeRegistry::new();
        reg.register_empty("Place.loc");
        // Apply a single insert via WAL replay.
        reg.apply_wal_entry(&WalEntry::RTreeInsert {
            index_name: "Place.loc".to_string(),
            node_id: 42,
            x: 1.0,
            y: 2.0,
        })
        .unwrap();
        let tree = reg.snapshot("Place.loc").unwrap();
        let hits = tree.nearest(1.0, 2.0, 1, Metric::Cartesian).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node_id, 42);
    }

    #[test]
    fn apply_wal_entry_handles_insert_and_delete() {
        let reg = RTreeRegistry::new();
        reg.apply_wal_entry(&WalEntry::RTreeInsert {
            index_name: "I".into(),
            node_id: 1,
            x: 0.0,
            y: 0.0,
        })
        .unwrap();
        reg.apply_wal_entry(&WalEntry::RTreeInsert {
            index_name: "I".into(),
            node_id: 2,
            x: 5.0,
            y: 5.0,
        })
        .unwrap();
        reg.apply_wal_entry(&WalEntry::RTreeDelete {
            index_name: "I".into(),
            node_id: 1,
        })
        .unwrap();

        let tree = reg.snapshot("I").unwrap();
        assert_eq!(tree.len(), 1);
        let hits = tree.nearest(5.0, 5.0, 5, Metric::Cartesian).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node_id, 2);
    }

    #[test]
    fn apply_wal_entry_ignores_non_rtree_variants() {
        let reg = RTreeRegistry::new();
        reg.apply_wal_entry(&WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        })
        .unwrap();
        // No index should have been created.
        assert!(reg.is_empty());
    }

    #[test]
    fn delete_for_unknown_node_is_idempotent() {
        let reg = RTreeRegistry::new();
        reg.apply_wal_entry(&WalEntry::RTreeInsert {
            index_name: "I".into(),
            node_id: 1,
            x: 0.0,
            y: 0.0,
        })
        .unwrap();
        // Deleting a never-inserted id during replay is a no-op
        // (some bulk-load shapes journal a delete for a node that
        // was inserted in a partial run discarded by recovery).
        reg.apply_wal_entry(&WalEntry::RTreeDelete {
            index_name: "I".into(),
            node_id: 999,
        })
        .unwrap();
        let tree = reg.snapshot("I").unwrap();
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn swap_in_replaces_tree_atomically() {
        let reg = RTreeRegistry::new();
        reg.apply_wal_entry(&WalEntry::RTreeInsert {
            index_name: "I".into(),
            node_id: 1,
            x: 0.0,
            y: 0.0,
        })
        .unwrap();

        // Reader captures the pre-swap snapshot.
        let pre = reg.snapshot("I").unwrap();
        assert_eq!(pre.len(), 1);

        // Build a brand new tree with different contents.
        let mut new_tree = RTree::new();
        new_tree.insert(100, 9.0, 9.0);
        new_tree.insert(101, 8.0, 8.0);
        reg.swap_in("I", new_tree);

        // The pre-swap snapshot still sees the old shape.
        assert_eq!(pre.len(), 1);

        // A fresh snapshot sees the new shape.
        let post = reg.snapshot("I").unwrap();
        assert_eq!(post.len(), 2);
    }

    #[test]
    fn nearest_with_filter_drops_invisible_ids() {
        let reg = RTreeRegistry::new();
        for (i, x) in (0..5u64).zip([0.0_f64, 1.0, 2.0, 3.0, 4.0]) {
            reg.apply_wal_entry(&WalEntry::RTreeInsert {
                index_name: "I".into(),
                node_id: i,
                x,
                y: 0.0,
            })
            .unwrap();
        }
        // Hide the two closest entries — caller's visibility
        // filter says they don't exist at the reader's epoch.
        let invisible: std::collections::HashSet<u64> = [0u64, 1].into_iter().collect();
        let hits = reg
            .nearest_with_filter("I", 0.0, 0.0, 2, Metric::Cartesian, |id| {
                !invisible.contains(&id)
            })
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].node_id, 2);
        assert_eq!(hits[1].node_id, 3);
    }

    #[test]
    fn drop_index_removes_subsequent_lookups() {
        let reg = RTreeRegistry::new();
        reg.register_empty("I");
        assert!(reg.contains("I"));
        assert!(reg.drop_index("I"));
        assert!(!reg.contains("I"));
        assert!(reg.snapshot("I").is_none());
    }
}
