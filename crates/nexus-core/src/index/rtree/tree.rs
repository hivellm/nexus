//! Mutable Hilbert R-tree
//! (phase6_rtree-index-core §3).
//!
//! In-memory tree backed by a `HashMap<page_id, Vec<ChildRef>>`.
//! Provides incremental insert + delete on top of the bulk-loaded
//! shape produced by [`super::packer::bulk_pack`].
//!
//! ## Insert (`O(log_b N)` expected)
//!
//! Descend from the root choosing the child whose bbox needs the
//! smallest area expansion to cover the new point. On a leaf page
//! at fanout cap, run the quadratic-split heuristic: pick the two
//! seeds that would waste the most area if grouped together,
//! then assign every other entry to whichever group it expands
//! less. The two resulting groups become two pages; the parent
//! link the new sibling in.
//!
//! ## Delete (`O(log_b N)` expected)
//!
//! Locate the leaf containing `node_id`, remove its entry. Leaf
//! underflow (count below
//! [`super::RTREE_MIN_FANOUT`] / 2) re-inserts every orphaned
//! entry through the regular insert path — simpler than B-tree
//! merging and competitive for read-heavy spatial workloads.
//!
//! ## Page-cache backing
//!
//! Pages live in memory for now. Slice §5 wires them into
//! `crate::page_cache::PageCache` so the same eviction logic the
//! B-tree uses applies here too. The public API of this module
//! does not change at that point — callers see the same
//! [`RTree::insert`] / [`RTree::delete`] / [`RTree::query_bbox`]
//! contract.

use std::collections::HashMap;

use super::packer::{PACK_TARGET_FANOUT, PackedTree, bounding_box, bulk_pack};
use super::page::{ChildRef, decode_page};
use super::{RTREE_MAX_FANOUT, RTREE_MIN_FANOUT};

/// Errors surfaced by the tree's mutation API. Read paths cannot
/// fail (a missing page id is treated as "no children" because the
/// invariants prevent that).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TreeError {
    /// `delete` was called for a node id that has never been
    /// inserted.
    #[error("R-tree delete: node id {0} not found")]
    NotFound(u64),
}

/// Mutable in-memory R-tree.
#[derive(Debug, Clone)]
pub struct RTree {
    pages: HashMap<u64, Vec<ChildRef>>,
    /// Each non-root page maps back to its parent so deletions can
    /// fix-up parent bboxes after a leaf shrink without re-walking
    /// the tree.
    parent_of: HashMap<u64, u64>,
    /// `level_of[page_id]` — 0 for leaves, 1 for inner pages whose
    /// children are leaves, … . Required so insert/split know
    /// whether to call themselves recursively.
    level_of: HashMap<u64, u8>,
    root_page_id: u64,
    next_page_id: u64,
    height: u8,
}

impl Default for RTree {
    fn default() -> Self {
        Self::new()
    }
}

impl RTree {
    /// Empty tree. `insert` lazily allocates the root on the first
    /// entry so callers don't pay for an unused root page.
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            parent_of: HashMap::new(),
            level_of: HashMap::new(),
            root_page_id: 0,
            next_page_id: 0,
            height: 0,
        }
    }

    /// Build an `RTree` from a [`PackedTree`] produced by
    /// [`bulk_pack`]. Decodes every page back to `Vec<ChildRef>`
    /// for in-memory mutation. Used by tests today; once §5 wires
    /// the page cache the tree will read pages on demand instead.
    pub fn from_packed(packed: &PackedTree) -> Self {
        let mut tree = RTree::new();
        if packed.pages.is_empty() {
            return tree;
        }
        for buf in &packed.pages {
            let (header, entries) =
                decode_page(buf).expect("packed page must decode (encoder/decoder symmetric)");
            let page_id = header.page_id as u64;
            tree.level_of.insert(page_id, header.level);
            // Inner-level entries reference child pages; record the
            // parent-of edge so leaf updates can repair their
            // bboxes.
            for e in &entries {
                if header.level > 0 {
                    tree.parent_of.insert(e.child_ptr, page_id);
                }
            }
            tree.pages.insert(page_id, entries);
            if page_id > tree.next_page_id {
                tree.next_page_id = page_id;
            }
        }
        tree.root_page_id = packed.root_page_id;
        tree.height = packed.height;
        tree
    }

    /// Number of leaf entries currently in the tree.
    pub fn len(&self) -> usize {
        self.collect_leaves().len()
    }

    /// `true` when no entries are indexed.
    pub fn is_empty(&self) -> bool {
        self.height == 0 || self.collect_leaves().is_empty()
    }

    /// Insert a 2-D point. Replaces any existing entry for the same
    /// `node_id` so the tree's "one entry per node id" invariant is
    /// preserved when callers re-index a moved node.
    pub fn insert(&mut self, node_id: u64, x: f64, y: f64) {
        // Re-index: drop the old entry first so a moved node ends
        // up exactly once.
        let _ = self.delete_internal(node_id);
        let entry = ChildRef::leaf_point_2d(node_id, x, y);
        if self.height == 0 {
            // Lazy root creation.
            let root = self.alloc_page();
            self.pages.insert(root, vec![entry]);
            self.level_of.insert(root, 0);
            self.root_page_id = root;
            self.height = 1;
            return;
        }
        let leaf_id = self.choose_leaf(self.root_page_id, &entry.bbox);
        self.insert_into_leaf(leaf_id, entry);
    }

    /// Delete the entry tagged `node_id`. Returns `Err` when the id
    /// isn't present so callers can distinguish "no-op" from
    /// "deleted".
    pub fn delete(&mut self, node_id: u64) -> Result<(), TreeError> {
        if self.delete_internal(node_id) {
            Ok(())
        } else {
            Err(TreeError::NotFound(node_id))
        }
    }

    /// Range search by bounding box. Returns every node id whose
    /// indexed point lies in `[min_x, max_x] × [min_y, max_y]`
    /// (closed intervals).
    pub fn query_bbox(&self, min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Vec<u64> {
        let target = [min_x, min_y, max_x, max_y];
        let mut out = Vec::new();
        if self.height == 0 {
            return out;
        }
        self.descend_bbox(self.root_page_id, &target, &mut out);
        out
    }

    /// `true` iff the tree carries an entry for `node_id`. O(N) —
    /// used by tests; production callers go through `query_bbox`.
    pub fn contains(&self, node_id: u64) -> bool {
        self.collect_leaves().iter().any(|(id, _)| *id == node_id)
    }

    /// Total page count across every level. Used by tests to
    /// confirm the structure matches expectations.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Tree height. 0 = empty. 1 = single-leaf root.
    pub fn height(&self) -> u8 {
        self.height
    }

    // --- internals ---------------------------------------------------

    fn alloc_page(&mut self) -> u64 {
        self.next_page_id += 1;
        self.next_page_id
    }

    fn choose_leaf(&self, page_id: u64, target: &[f64; 4]) -> u64 {
        let level = *self.level_of.get(&page_id).unwrap_or(&0);
        if level == 0 {
            return page_id;
        }
        // Pick the child entry whose bbox needs the smallest area
        // increase to cover `target`. Ties break on smaller area.
        let entries = self.pages.get(&page_id).expect("inner page exists");
        let mut best: Option<(u64, f64, f64)> = None;
        for e in entries {
            let cur_area = bbox_area(&e.bbox);
            let new_area = bbox_area(&union_bbox(&e.bbox, target));
            let delta = new_area - cur_area;
            let candidate = (e.child_ptr, delta, cur_area);
            best = match best {
                None => Some(candidate),
                Some(prev) if delta < prev.1 || (delta == prev.1 && cur_area < prev.2) => {
                    Some(candidate)
                }
                _ => best,
            };
        }
        let chosen = best.expect("inner page non-empty by invariant").0;
        self.choose_leaf(chosen, target)
    }

    fn insert_into_leaf(&mut self, leaf_id: u64, entry: ChildRef) {
        let entries = self.pages.entry(leaf_id).or_default();
        entries.push(entry);
        if entries.len() > usize::from(RTREE_MAX_FANOUT) {
            self.split_leaf(leaf_id);
        } else {
            // Just a bbox-fixup along the parent chain — no split.
            self.refresh_parent_bbox(leaf_id);
        }
    }

    fn split_leaf(&mut self, leaf_id: u64) {
        // Drain the overfull leaf and partition with the quadratic
        // split heuristic. The two resulting groups become the
        // original page id (group A) and a freshly allocated
        // sibling (group B).
        let entries = self
            .pages
            .remove(&leaf_id)
            .expect("split target page exists");
        let (a, b) = quadratic_split(entries);
        let sibling_id = self.alloc_page();
        let level = *self.level_of.get(&leaf_id).unwrap_or(&0);
        self.level_of.insert(sibling_id, level);
        self.pages.insert(leaf_id, a);
        self.pages.insert(sibling_id, b);

        // Insert the new sibling pointer into the leaf's parent.
        let new_entry = ChildRef::inner(
            sibling_id,
            u64::from(level),
            bounding_box(self.pages.get(&sibling_id).expect("split B exists")),
        );

        if self.root_page_id == leaf_id {
            // Old root became a child of a brand new root.
            let new_root = self.alloc_page();
            let old_root_bbox = bounding_box(self.pages.get(&leaf_id).expect("split A exists"));
            self.pages.insert(
                new_root,
                vec![
                    ChildRef::inner(leaf_id, u64::from(level), old_root_bbox),
                    new_entry,
                ],
            );
            self.level_of.insert(new_root, level + 1);
            self.parent_of.insert(leaf_id, new_root);
            self.parent_of.insert(sibling_id, new_root);
            self.root_page_id = new_root;
            self.height += 1;
        } else {
            let parent_id = *self
                .parent_of
                .get(&leaf_id)
                .expect("non-root page has a recorded parent");
            self.parent_of.insert(sibling_id, parent_id);
            // Refresh parent's bbox for the original page since its
            // contents shrank, then push the new sibling pointer.
            self.refresh_parent_bbox(leaf_id);
            let parent_entries = self.pages.entry(parent_id).or_default();
            parent_entries.push(new_entry);
            if parent_entries.len() > usize::from(RTREE_MAX_FANOUT) {
                self.split_leaf(parent_id);
            } else {
                self.refresh_parent_bbox(parent_id);
            }
        }
    }

    fn refresh_parent_bbox(&mut self, page_id: u64) {
        let Some(parent_id) = self.parent_of.get(&page_id).copied() else {
            return; // Root has no parent.
        };
        let new_bbox = match self.pages.get(&page_id) {
            Some(entries) if !entries.is_empty() => bounding_box(entries),
            _ => return,
        };
        if let Some(parent_entries) = self.pages.get_mut(&parent_id) {
            for e in parent_entries.iter_mut() {
                if e.child_ptr == page_id {
                    e.bbox = new_bbox;
                    break;
                }
            }
        }
        self.refresh_parent_bbox(parent_id);
    }

    fn delete_internal(&mut self, node_id: u64) -> bool {
        if self.height == 0 {
            return false;
        }
        let leaves: Vec<(u64, u64)> = self
            .pages
            .iter()
            .filter(|(pid, _)| *self.level_of.get(*pid).unwrap_or(&255) == 0)
            .flat_map(|(pid, entries)| entries.iter().map(move |e| (*pid, e.child_ptr)))
            .collect();
        let Some((leaf_id, _)) = leaves.into_iter().find(|(_, id)| *id == node_id) else {
            return false;
        };
        let entries = self.pages.get_mut(&leaf_id).expect("leaf page present");
        let before = entries.len();
        entries.retain(|e| e.child_ptr != node_id);
        if entries.len() == before {
            return false;
        }

        // Underflow handling: if the page is now too small AND it's
        // not the lone root, drain it and re-insert the survivors
        // through the public path. Simpler than B-tree merging.
        let underflow = entries.len() < usize::from(RTREE_MIN_FANOUT) / 2;
        let is_root = leaf_id == self.root_page_id;
        if underflow && !is_root {
            let orphans = self
                .pages
                .remove(&leaf_id)
                .expect("leaf to drain")
                .into_iter()
                .map(|e| (e.child_ptr, e.bbox[0], e.bbox[1]))
                .collect::<Vec<_>>();
            self.detach_leaf(leaf_id);
            for (id, x, y) in orphans {
                self.insert(id, x, y);
            }
        } else {
            self.refresh_parent_bbox(leaf_id);
        }
        true
    }

    fn detach_leaf(&mut self, leaf_id: u64) {
        let parent_id = match self.parent_of.remove(&leaf_id) {
            Some(p) => p,
            None => return,
        };
        if let Some(parent_entries) = self.pages.get_mut(&parent_id) {
            parent_entries.retain(|e| e.child_ptr != leaf_id);
        }
        self.level_of.remove(&leaf_id);
        self.refresh_parent_bbox(parent_id);
        // If the parent collapsed to zero children, prune it too.
        if let Some(parent_entries) = self.pages.get(&parent_id) {
            if parent_entries.is_empty() && parent_id != self.root_page_id {
                self.pages.remove(&parent_id);
                self.detach_leaf(parent_id);
            }
        }
    }

    fn descend_bbox(&self, page_id: u64, target: &[f64; 4], out: &mut Vec<u64>) {
        let level = *self.level_of.get(&page_id).unwrap_or(&0);
        let Some(entries) = self.pages.get(&page_id) else {
            return;
        };
        for e in entries {
            if !e.intersects(target) {
                continue;
            }
            if level == 0 {
                out.push(e.child_ptr);
            } else {
                self.descend_bbox(e.child_ptr, target, out);
            }
        }
    }

    fn collect_leaves(&self) -> Vec<(u64, [f64; 4])> {
        let mut out = Vec::new();
        if self.height == 0 {
            return out;
        }
        self.collect_leaves_rec(self.root_page_id, &mut out);
        out
    }

    fn collect_leaves_rec(&self, page_id: u64, out: &mut Vec<(u64, [f64; 4])>) {
        let level = *self.level_of.get(&page_id).unwrap_or(&0);
        let Some(entries) = self.pages.get(&page_id) else {
            return;
        };
        if level == 0 {
            for e in entries {
                out.push((e.child_ptr, e.bbox));
            }
        } else {
            for e in entries {
                self.collect_leaves_rec(e.child_ptr, out);
            }
        }
    }
}

// --- helpers --------------------------------------------------------

fn bbox_area(b: &[f64; 4]) -> f64 {
    let dx = (b[2] - b[0]).max(0.0);
    let dy = (b[3] - b[1]).max(0.0);
    dx * dy
}

fn union_bbox(a: &[f64; 4], b: &[f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// Quadratic split (Guttman 1984) — pick the seed pair with the
/// largest "wasted" area when grouped, then assign each remaining
/// entry to whichever group it expands less. Falls back to balanced
/// split if there's a tie.
fn quadratic_split(entries: Vec<ChildRef>) -> (Vec<ChildRef>, Vec<ChildRef>) {
    debug_assert!(
        entries.len() >= 2,
        "quadratic_split requires at least two entries"
    );
    // Pick seeds.
    let mut worst_pair = (0usize, 1usize);
    let mut worst_waste = f64::NEG_INFINITY;
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            let union = union_bbox(&entries[i].bbox, &entries[j].bbox);
            let waste =
                bbox_area(&union) - bbox_area(&entries[i].bbox) - bbox_area(&entries[j].bbox);
            if waste > worst_waste {
                worst_waste = waste;
                worst_pair = (i, j);
            }
        }
    }
    let (seed_a, seed_b) = worst_pair;
    let mut group_a: Vec<ChildRef> = Vec::with_capacity(entries.len() / 2 + 1);
    let mut group_b: Vec<ChildRef> = Vec::with_capacity(entries.len() / 2 + 1);
    let mut bbox_a;
    let mut bbox_b;
    let mut remaining: Vec<ChildRef> = Vec::with_capacity(entries.len());
    for (idx, e) in entries.into_iter().enumerate() {
        if idx == seed_a {
            bbox_a_init(&e, &mut group_a);
            continue;
        }
        if idx == seed_b {
            bbox_b_init(&e, &mut group_b);
            continue;
        }
        remaining.push(e);
    }
    bbox_a = group_a[0].bbox;
    bbox_b = group_b[0].bbox;

    let min_per_group = usize::from(RTREE_MIN_FANOUT) / 2;
    while !remaining.is_empty() {
        // Force-fill if one group would fall below the minimum.
        let needed_a = min_per_group.saturating_sub(group_a.len());
        let needed_b = min_per_group.saturating_sub(group_b.len());
        if needed_a >= remaining.len() {
            for e in remaining.drain(..) {
                bbox_a = union_bbox(&bbox_a, &e.bbox);
                group_a.push(e);
            }
            break;
        }
        if needed_b >= remaining.len() {
            for e in remaining.drain(..) {
                bbox_b = union_bbox(&bbox_b, &e.bbox);
                group_b.push(e);
            }
            break;
        }

        // Pick the next entry to assign — the one with the largest
        // gap between the area expansion of A and B.
        let mut chosen = 0usize;
        let mut best_gap = f64::NEG_INFINITY;
        let mut chosen_into_a = true;
        for (i, e) in remaining.iter().enumerate() {
            let exp_a = bbox_area(&union_bbox(&bbox_a, &e.bbox)) - bbox_area(&bbox_a);
            let exp_b = bbox_area(&union_bbox(&bbox_b, &e.bbox)) - bbox_area(&bbox_b);
            let gap = (exp_a - exp_b).abs();
            if gap > best_gap {
                best_gap = gap;
                chosen = i;
                chosen_into_a = exp_a < exp_b;
            }
        }
        let entry = remaining.remove(chosen);
        if chosen_into_a {
            bbox_a = union_bbox(&bbox_a, &entry.bbox);
            group_a.push(entry);
        } else {
            bbox_b = union_bbox(&bbox_b, &entry.bbox);
            group_b.push(entry);
        }
    }
    (group_a, group_b)
}

fn bbox_a_init(e: &ChildRef, group_a: &mut Vec<ChildRef>) {
    group_a.push(*e);
}

fn bbox_b_init(e: &ChildRef, group_b: &mut Vec<ChildRef>) {
    group_b.push(*e);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_ids(tree: &RTree) -> Vec<u64> {
        let mut ids: Vec<u64> = tree
            .collect_leaves()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn insert_into_empty_lazily_creates_root() {
        let mut tree = RTree::new();
        assert_eq!(tree.height(), 0);
        tree.insert(7, 1.0, 2.0);
        assert_eq!(tree.height(), 1);
        assert!(tree.contains(7));
        assert_eq!(tree.query_bbox(0.0, 0.0, 5.0, 5.0), vec![7]);
    }

    #[test]
    fn insert_then_query_bbox_returns_all_in_range() {
        let mut tree = RTree::new();
        for i in 0..50u64 {
            tree.insert(i, f64::from(i as i32), f64::from((i * 2) as i32));
        }
        let inside = tree.query_bbox(10.0, 0.0, 20.0, 100.0);
        let mut ids = inside.clone();
        ids.sort_unstable();
        let expected: Vec<u64> = (10..=20u64).collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn insert_overflow_promotes_root_via_split() {
        // RTREE_MAX_FANOUT + 1 forces a split out of the lone-leaf
        // root. The new root sits at level 1 and the height
        // increments to 2.
        let mut tree = RTree::new();
        for i in 0..(RTREE_MAX_FANOUT as u64 + 1) {
            tree.insert(
                i,
                ((i.wrapping_mul(2654435761) >> 16) & 0xff) as f64,
                ((i.wrapping_mul(40503) >> 8) & 0xff) as f64,
            );
        }
        assert_eq!(tree.height(), 2);
        assert!(tree.page_count() >= 3, "root + at least two leaves");
        // Every inserted id must be reachable through a wide range
        // query.
        let visible = tree.query_bbox(-1.0, -1.0, 1000.0, 1000.0);
        let mut ids = visible;
        ids.sort_unstable();
        assert_eq!(ids.len(), RTREE_MAX_FANOUT as usize + 1);
    }

    #[test]
    fn delete_removes_entry_from_subsequent_queries() {
        let mut tree = RTree::new();
        for i in 0..30u64 {
            tree.insert(i, f64::from(i as i32), 0.0);
        }
        tree.delete(15).unwrap();
        let visible = tree.query_bbox(-1.0, -1.0, 100.0, 1.0);
        assert!(!visible.contains(&15));
        assert_eq!(visible.len(), 29);
        assert!(!tree.contains(15));
    }

    #[test]
    fn delete_unknown_id_is_a_typed_error() {
        let mut tree = RTree::new();
        tree.insert(1, 0.0, 0.0);
        let err = tree.delete(99).unwrap_err();
        assert_eq!(err, TreeError::NotFound(99));
    }

    #[test]
    fn delete_underflow_reinserts_orphans() {
        // Build a tree at height >= 2, then delete enough entries
        // from one leaf to underflow it. Every survivor across the
        // whole tree should still be reachable afterwards.
        let mut tree = RTree::new();
        let n: u64 = 200;
        for i in 0..n {
            tree.insert(i, f64::from((i % 16) as i32), f64::from((i / 16) as i32));
        }
        for victim in 0..40u64 {
            tree.delete(victim).unwrap();
        }
        let surviving_ids: Vec<u64> = (40..n).collect();
        let mut visible = tree.query_bbox(-1.0, -1.0, 100.0, 100.0);
        visible.sort_unstable();
        assert_eq!(visible, surviving_ids);
        assert_eq!(collect_ids(&tree), surviving_ids);
    }

    #[test]
    fn reinserting_same_node_id_replaces_entry() {
        let mut tree = RTree::new();
        tree.insert(1, 0.0, 0.0);
        tree.insert(1, 50.0, 50.0); // same id moves
        // Old position must be empty.
        assert!(tree.query_bbox(-1.0, -1.0, 1.0, 1.0).is_empty());
        // New position has it.
        assert_eq!(tree.query_bbox(45.0, 45.0, 55.0, 55.0), vec![1]);
        // Total count stays at one.
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn from_packed_round_trips_through_query() {
        // Bulk-load + open as a mutable tree, then query.
        let mut leaves: Vec<ChildRef> = (0..400u64)
            .map(|i| {
                ChildRef::leaf_point_2d(i, f64::from((i % 20) as i32), f64::from((i / 20) as i32))
            })
            .collect();
        super::super::hilbert::sort_by_hilbert_2d(&mut leaves, 8, |c| {
            (c.bbox[0], c.bbox[1], c.child_ptr)
        });
        let packed = bulk_pack(&leaves);
        let tree = RTree::from_packed(&packed);
        assert_eq!(tree.len(), 400);
        let visible = tree.query_bbox(-1.0, -1.0, 100.0, 100.0);
        assert_eq!(visible.len(), 400);
    }
}
