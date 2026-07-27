//! Authoritative in-memory adjacency index owned by [`RecordStore`].
//!
//! Maps each node to the LIVE relationship ids incident on it, in both
//! directions. It exists because `NodeRecord` heads only an OUTGOING chain
//! (`first_rel_ptr`, walked via `next_src_ptr`) — the store has no reverse
//! adjacency at all, so "who points at node N?" used to cost a full
//! O(total relationships) scan on the node-delete guard and on
//! `DETACH DELETE`.
//!
//! # What makes it authoritative
//!
//! It is maintained in exactly one place — [`super::RecordStore::write_rel`],
//! the single funnel every relationship-record mutation passes through — plus
//! [`super::RecordStore::clear_all`], which resets the store wholesale. No
//! caller opts in, so no write path can desync it by forgetting to. This is
//! precisely the property `cache::RelationshipIndex` lacks (the executor
//! `CREATE` operator and the bulk loader write edges straight to the store
//! without notifying it), which is why that one is only ever a hint.
//!
//! Endpoints are immutable once written — nothing in the codebase assigns
//! `src_id`/`dst_id` on an existing record — so a write only ever has to add
//! or remove the record's own id, never move it between nodes.
//!
//! See `docs/analysis/store-adjacency-index/`.

use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Per-direction adjacency: `node_id → {live relationship ids}`.
type Adjacency = HashMap<u64, HashSet<u64>>;

#[derive(Debug, Default)]
struct Inner {
    /// Keyed by `src_id`: relationships the node is the source of.
    outgoing: Adjacency,
    /// Keyed by `dst_id`: relationships pointing AT the node — the reverse
    /// adjacency the record format does not provide.
    incoming: Adjacency,
}

/// Live-relationship adjacency in both directions, shared by every clone of
/// the owning [`RecordStore`] (clones share storage, so they must share the
/// index derived from it).
#[derive(Debug, Clone, Default)]
pub struct AdjacencyIndex {
    inner: Arc<RwLock<Inner>>,
}

impl AdjacencyIndex {
    /// An empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `rel_id` as a live edge from `src` to `dst`.
    ///
    /// Idempotent: `write_rel` is called again on an already-live record every
    /// time `create_relationship` patches the previous edge's `next_src_ptr`,
    /// and re-inserting into a set is a no-op. A self-loop (`src == dst`)
    /// lands in both directions and is de-duplicated by [`Self::connected`].
    pub fn insert(&self, rel_id: u64, src: u64, dst: u64) {
        let mut inner = self.inner.write();
        inner.outgoing.entry(src).or_default().insert(rel_id);
        inner.incoming.entry(dst).or_default().insert(rel_id);
    }

    /// Drop `rel_id` from both directions — the record was marked deleted.
    /// Idempotent, and prunes the node entry once its last edge is gone so a
    /// churning graph does not accumulate empty sets.
    pub fn remove(&self, rel_id: u64, src: u64, dst: u64) {
        let mut inner = self.inner.write();
        if let Some(set) = inner.outgoing.get_mut(&src) {
            set.remove(&rel_id);
            if set.is_empty() {
                inner.outgoing.remove(&src);
            }
        }
        if let Some(set) = inner.incoming.get_mut(&dst) {
            set.remove(&rel_id);
            if set.is_empty() {
                inner.incoming.remove(&dst);
            }
        }
    }

    /// Live relationship ids the node is the SOURCE of, ascending.
    #[must_use]
    pub fn outgoing(&self, node_id: u64) -> Vec<u64> {
        Self::sorted(self.inner.read().outgoing.get(&node_id))
    }

    /// Live relationship ids pointing AT the node, ascending.
    #[must_use]
    pub fn incoming(&self, node_id: u64) -> Vec<u64> {
        Self::sorted(self.inner.read().incoming.get(&node_id))
    }

    /// Every live relationship id incident on the node in either direction,
    /// ascending and de-duplicated (a self-loop appears once).
    #[must_use]
    pub fn connected(&self, node_id: u64) -> Vec<u64> {
        let inner = self.inner.read();
        let mut ids: Vec<u64> = inner
            .outgoing
            .get(&node_id)
            .into_iter()
            .chain(inner.incoming.get(&node_id))
            .flatten()
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Whether ANY live relationship is incident on the node — O(1), without
    /// materialising the id list.
    #[must_use]
    pub fn has_any(&self, node_id: u64) -> bool {
        let inner = self.inner.read();
        inner.outgoing.contains_key(&node_id) || inner.incoming.contains_key(&node_id)
    }

    /// Drop everything. Used by `RecordStore::clear_all`, which re-maps the
    /// record files wholesale and therefore bypasses `write_rel`.
    pub fn clear(&self) {
        let mut inner = self.inner.write();
        inner.outgoing.clear();
        inner.incoming.clear();
    }

    /// Number of nodes carrying at least one live outgoing / incoming edge.
    /// Diagnostics only.
    #[must_use]
    pub fn node_count(&self) -> (usize, usize) {
        let inner = self.inner.read();
        (inner.outgoing.len(), inner.incoming.len())
    }

    fn sorted(set: Option<&HashSet<u64>>) -> Vec<u64> {
        let mut ids: Vec<u64> = set.map(|s| s.iter().copied().collect()).unwrap_or_default();
        ids.sort_unstable();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_is_visible_in_both_directions() {
        let idx = AdjacencyIndex::new();
        idx.insert(7, 1, 2);
        assert_eq!(idx.outgoing(1), vec![7]);
        assert_eq!(idx.incoming(2), vec![7]);
        assert!(idx.outgoing(2).is_empty(), "2 sources nothing");
        assert!(idx.incoming(1).is_empty(), "nothing points at 1");
        assert!(idx.has_any(1) && idx.has_any(2));
    }

    #[test]
    fn insert_is_idempotent() {
        let idx = AdjacencyIndex::new();
        idx.insert(7, 1, 2);
        idx.insert(7, 1, 2);
        assert_eq!(idx.outgoing(1), vec![7], "a re-write must not duplicate");
        assert_eq!(idx.incoming(2), vec![7]);
    }

    #[test]
    fn remove_prunes_the_node_entry() {
        let idx = AdjacencyIndex::new();
        idx.insert(7, 1, 2);
        idx.remove(7, 1, 2);
        assert!(!idx.has_any(1), "1 has no live edge left");
        assert!(!idx.has_any(2), "2 has no live edge left");
        assert!(idx.connected(1).is_empty());
        // Removing again is a no-op, not a panic.
        idx.remove(7, 1, 2);
    }

    #[test]
    fn self_loop_appears_once_in_connected() {
        let idx = AdjacencyIndex::new();
        idx.insert(3, 5, 5);
        assert_eq!(idx.outgoing(5), vec![3]);
        assert_eq!(idx.incoming(5), vec![3]);
        assert_eq!(idx.connected(5), vec![3], "a self-loop is one edge");
    }

    #[test]
    fn connected_merges_and_sorts_both_directions() {
        let idx = AdjacencyIndex::new();
        idx.insert(9, 1, 2);
        idx.insert(4, 3, 2);
        idx.insert(6, 2, 4);
        assert_eq!(idx.connected(2), vec![4, 6, 9]);
    }

    #[test]
    fn clones_share_one_index() {
        let idx = AdjacencyIndex::new();
        let clone = idx.clone();
        idx.insert(1, 10, 20);
        assert_eq!(clone.incoming(20), vec![1], "clones must share state");
        clone.clear();
        assert!(!idx.has_any(10), "clear on one clone clears all");
    }
}
