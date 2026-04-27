//! Spatial-query operators on the in-memory R-tree
//! (phase6_rtree-index-core §4).
//!
//! Three public entry points layered on top of [`super::tree::RTree`]:
//!
//! - **Range bbox** — already lives on `RTree::query_bbox` (§3); the
//!   helpers here just expose the underlying intersect/contains
//!   primitives so external callers don't reach into the tree's
//!   private interface.
//! - **k-NN** — priority-queue walk that visits inner pages in
//!   increasing bbox-to-point distance order and stops after `k`
//!   leaves are popped. O(log_b N + k) page reads versus the
//!   linear `O(N)` scan of the grid backend.
//! - **Within-distance** — Euclidean by default; the great-circle
//!   variant lives behind a `WGS84` flag for callers wiring in the
//!   `point.withinDistance` Cypher predicate.
//!
//! ## Distance metric
//!
//! [`Metric`] is intentionally tiny — Cartesian for now, with a
//! reserved WGS-84 variant so `point.distance(p1, p2, 'wgs-84')`
//! has a code path to grow into. The geodesic helpers land
//! alongside the planner-seek slice; this module's WGS-84 branch
//! returns `Err(Wgs84Unsupported)` until then so misrouted callers
//! see a typed error instead of a silent zero distance.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::page::ChildRef;
use super::tree::RTree;

/// Distance metric for `nearest` / `within_distance`. Cartesian
/// matches the Cypher 'cartesian' CRS; WGS-84 lands once the
/// geodesic helpers are wired (slice §7 has the parser glue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    /// Plain 2-D Euclidean distance: `sqrt(dx^2 + dy^2)`.
    Cartesian,
    /// WGS-84 great-circle distance. Currently unimplemented; the
    /// search path returns `Err(Wgs84Unsupported)` so callers can
    /// surface a typed error to the user.
    Wgs84,
}

/// Errors specific to the search routines. The tree itself never
/// fails on read; this enum only fires when the caller asks for an
/// unsupported metric.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SearchError {
    /// Caller requested WGS-84 but the geodesic helpers haven't
    /// been wired up yet. Parser-side validation catches this in
    /// the executor; the runtime guard exists for hand-built
    /// callers that bypass the planner.
    #[error("WGS-84 metric is not implemented yet (use Metric::Cartesian)")]
    Wgs84Unsupported,
}

/// `nearest(p, k)` result row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestHit {
    /// Owning node id.
    pub node_id: u64,
    /// Distance from the query point under [`Metric::Cartesian`].
    pub distance: f64,
}

/// Squared Euclidean distance from `p` to a 2-D bbox. `0.0` when
/// `p` lies inside the bbox; otherwise the squared distance to the
/// nearest edge.
pub fn bbox_to_point_sq(bbox: &[f64; 4], px: f64, py: f64) -> f64 {
    let dx = if px < bbox[0] {
        bbox[0] - px
    } else if px > bbox[2] {
        px - bbox[2]
    } else {
        0.0
    };
    let dy = if py < bbox[1] {
        bbox[1] - py
    } else if py > bbox[3] {
        py - bbox[3]
    } else {
        0.0
    };
    dx * dx + dy * dy
}

/// `true` when `inner` is fully contained in `outer` (closed).
pub fn bbox_contains(outer: &[f64; 4], inner: &[f64; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

/// `true` when `a` and `b` overlap (closed intervals). Mirrors
/// `ChildRef::intersects` — exposed at module scope so callers
/// that only have raw bboxes can use it.
pub fn bbox_intersects(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1]
}

/// Min-heap entry. We invert `Ord` so `BinaryHeap` (max-heap)
/// behaves as a min-heap on `priority`.
#[derive(Debug, Clone, Copy)]
struct HeapItem {
    /// Squared bbox-to-point distance for inner pages, or squared
    /// point distance for leaf entries.
    priority: f64,
    /// `true` for an inner-page slot (descend), `false` for a
    /// leaf-entry slot (emit).
    is_inner: bool,
    /// Page id for inner slots; `node_id` for leaf slots.
    payload: u64,
    /// Disambiguator for stable tie-breaking. Heaps don't promise
    /// FIFO order on equal priorities; this counter does.
    seq: u64,
}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.seq == other.seq
    }
}
impl Eq for HeapItem {}
impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: smaller priority wins, so reverse the natural
        // f64 order. NaN priorities should not appear (callers
        // pass finite coordinates), but we treat them as worst.
        match other.priority.partial_cmp(&self.priority) {
            Some(o) if o != Ordering::Equal => o,
            _ => other.seq.cmp(&self.seq),
        }
    }
}

impl RTree {
    /// k-Nearest-neighbour walk. Returns up to `k` entries ordered
    /// by ascending distance under the requested [`Metric`]. Ties
    /// break on `node_id` ascending so the order is deterministic.
    ///
    /// The walk uses a min-heap keyed on bbox-to-point distance.
    /// When the heap pops a leaf entry, that entry's distance is
    /// guaranteed minimum across every still-buffered slot so it
    /// can be emitted immediately. Inner pages get expanded; their
    /// children are pushed back onto the heap with their own
    /// bbox-to-point priorities. The walk stops once `k` leaves
    /// have been emitted.
    pub fn nearest(
        &self,
        px: f64,
        py: f64,
        k: usize,
        metric: Metric,
    ) -> Result<Vec<NearestHit>, SearchError> {
        if k == 0 || self.height() == 0 {
            return Ok(Vec::new());
        }
        if matches!(metric, Metric::Wgs84) {
            return Err(SearchError::Wgs84Unsupported);
        }

        let mut hits: Vec<NearestHit> = Vec::with_capacity(k);
        let mut heap: BinaryHeap<HeapItem> = BinaryHeap::new();
        let mut seq: u64 = 0;
        heap.push(HeapItem {
            priority: 0.0,
            is_inner: true,
            payload: self.root_page_id_pub(),
            seq,
        });
        seq += 1;

        while let Some(top) = heap.pop() {
            if !top.is_inner {
                hits.push(NearestHit {
                    node_id: top.payload,
                    distance: top.priority.sqrt(),
                });
                if hits.len() >= k {
                    break;
                }
                continue;
            }
            let page_id = top.payload;
            let level = self.level_for_pub(page_id);
            let entries = self.entries_for_pub(page_id);
            for entry in entries {
                let pri = bbox_to_point_sq(&entry.bbox, px, py);
                let payload = entry.child_ptr;
                let is_inner = level != 0;
                heap.push(HeapItem {
                    priority: pri,
                    is_inner,
                    payload,
                    seq,
                });
                seq += 1;
            }
        }

        // Determinism: sort by (distance, node_id) so callers see a
        // stable order even when the heap broke a tie arbitrarily.
        hits.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(Ordering::Equal)
                .then(a.node_id.cmp(&b.node_id))
        });
        hits.truncate(k);
        Ok(hits)
    }

    /// All entries within `max_distance` of `(px, py)` under
    /// [`Metric::Cartesian`]. Returns ids only — distances are
    /// available via [`RTree::nearest`] when the caller needs them.
    /// Order: ascending distance, ties on `node_id` ascending.
    pub fn within_distance(
        &self,
        px: f64,
        py: f64,
        max_distance: f64,
        metric: Metric,
    ) -> Result<Vec<u64>, SearchError> {
        if self.height() == 0 || max_distance < 0.0 {
            return Ok(Vec::new());
        }
        if matches!(metric, Metric::Wgs84) {
            return Err(SearchError::Wgs84Unsupported);
        }

        let max_sq = max_distance * max_distance;
        let mut out: Vec<(f64, u64)> = Vec::new();
        let mut stack: Vec<u64> = vec![self.root_page_id_pub()];
        while let Some(page_id) = stack.pop() {
            let level = self.level_for_pub(page_id);
            let entries = self.entries_for_pub(page_id);
            for entry in entries {
                let pri = bbox_to_point_sq(&entry.bbox, px, py);
                if pri > max_sq {
                    continue;
                }
                if level == 0 {
                    // Leaf entry's bbox is the point itself, so
                    // `pri` is the actual squared distance.
                    out.push((pri, entry.child_ptr));
                } else {
                    stack.push(entry.child_ptr);
                }
            }
        }
        out.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        Ok(out.into_iter().map(|(_, id)| id).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tree::RTree;
    use super::*;

    fn build_grid(side: usize) -> RTree {
        let mut tree = RTree::new();
        let mut id = 0u64;
        for x in 0..side {
            for y in 0..side {
                tree.insert(id, x as f64, y as f64);
                id += 1;
            }
        }
        tree
    }

    #[test]
    fn bbox_to_point_sq_zero_when_point_inside() {
        assert_eq!(bbox_to_point_sq(&[0.0, 0.0, 10.0, 10.0], 5.0, 5.0), 0.0);
    }

    #[test]
    fn bbox_to_point_sq_handles_each_quadrant() {
        let b = [10.0, 20.0, 30.0, 40.0];
        // Below-left.
        assert!((bbox_to_point_sq(&b, 0.0, 0.0) - (100.0 + 400.0)).abs() < 1e-9);
        // Above-right.
        assert!((bbox_to_point_sq(&b, 35.0, 50.0) - (25.0 + 100.0)).abs() < 1e-9);
        // Directly to the right.
        assert!((bbox_to_point_sq(&b, 40.0, 30.0) - 100.0).abs() < 1e-9);
        // Directly above.
        assert!((bbox_to_point_sq(&b, 20.0, 50.0) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn nearest_on_empty_tree_returns_empty() {
        let tree = RTree::new();
        let hits = tree.nearest(0.0, 0.0, 5, Metric::Cartesian).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn nearest_k_zero_short_circuits() {
        let mut tree = RTree::new();
        tree.insert(1, 0.0, 0.0);
        let hits = tree.nearest(0.0, 0.0, 0, Metric::Cartesian).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn nearest_returns_k_closest_in_ascending_distance() {
        let tree = build_grid(8); // 64 points
        let hits = tree.nearest(3.5, 3.5, 5, Metric::Cartesian).unwrap();
        assert_eq!(hits.len(), 5);
        // Confirmed-closest 4 cells around (3.5, 3.5) tie at sqrt(0.5).
        let expected_d = (0.5_f64).sqrt();
        for h in &hits[..4] {
            assert!(
                (h.distance - expected_d).abs() < 1e-9,
                "expected {expected_d}, got {}",
                h.distance
            );
        }
        // Strict ascending order overall.
        for w in hits.windows(2) {
            assert!(w[0].distance <= w[1].distance);
        }
    }

    #[test]
    fn nearest_breaks_ties_on_node_id_ascending() {
        // Two coincident points at the same distance; the
        // tie-breaker must put the smaller node_id first.
        let mut tree = RTree::new();
        tree.insert(50, 0.0, 0.0);
        tree.insert(7, 0.0, 0.0);
        let hits = tree.nearest(0.0, 0.0, 2, Metric::Cartesian).unwrap();
        assert_eq!(hits[0].node_id, 7);
        assert_eq!(hits[1].node_id, 50);
    }

    #[test]
    fn nearest_with_split_root_still_sees_every_entry() {
        let mut tree = RTree::new();
        for i in 0..(super::super::RTREE_MAX_FANOUT as u64 + 50) {
            tree.insert(i, f64::from(i as u32), 0.0);
        }
        let hits = tree
            .nearest(
                0.0,
                0.0,
                super::super::RTREE_MAX_FANOUT as usize + 50,
                Metric::Cartesian,
            )
            .unwrap();
        assert_eq!(hits.len(), super::super::RTREE_MAX_FANOUT as usize + 50);
    }

    #[test]
    fn nearest_rejects_wgs84_with_typed_error() {
        let mut tree = RTree::new();
        tree.insert(1, 0.0, 0.0);
        let err = tree.nearest(0.0, 0.0, 1, Metric::Wgs84).unwrap_err();
        assert_eq!(err, SearchError::Wgs84Unsupported);
    }

    #[test]
    fn within_distance_filters_by_radius() {
        let tree = build_grid(10); // 100 points on integer grid 0..9
        let ids = tree
            .within_distance(0.0, 0.0, 1.5, Metric::Cartesian)
            .unwrap();
        // Within a circle of radius 1.5 around origin: (0,0),
        // (0,1), (1,0), (1,1) — distances 0, 1, 1, sqrt(2)~1.414.
        // (0,2)/(2,0) at exactly 2.0 are out.
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn within_distance_zero_radius_returns_only_exact_match() {
        let mut tree = RTree::new();
        tree.insert(1, 5.0, 5.0);
        tree.insert(2, 5.001, 5.0);
        let ids = tree
            .within_distance(5.0, 5.0, 0.0, Metric::Cartesian)
            .unwrap();
        assert_eq!(ids, vec![1]);
    }

    #[test]
    fn within_distance_negative_radius_is_empty() {
        let mut tree = RTree::new();
        tree.insert(1, 0.0, 0.0);
        let ids = tree
            .within_distance(0.0, 0.0, -1.0, Metric::Cartesian)
            .unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn within_distance_orders_by_ascending_distance_then_id() {
        let mut tree = RTree::new();
        tree.insert(3, 0.0, 0.0);
        tree.insert(1, 1.0, 0.0);
        tree.insert(2, 0.0, 1.0); // same distance as id=1
        let ids = tree
            .within_distance(0.0, 0.0, 5.0, Metric::Cartesian)
            .unwrap();
        // id=3 at distance 0; then ids 1 and 2 tied at 1.0 with
        // smaller id first.
        assert_eq!(ids, vec![3, 1, 2]);
    }

    #[test]
    fn bbox_contains_and_intersects_smoke() {
        let outer = [0.0, 0.0, 10.0, 10.0];
        assert!(bbox_contains(&outer, &[1.0, 1.0, 2.0, 2.0]));
        assert!(!bbox_contains(&outer, &[1.0, 1.0, 11.0, 2.0]));
        assert!(bbox_intersects(&outer, &[5.0, 5.0, 15.0, 15.0]));
        assert!(!bbox_intersects(&outer, &[20.0, 20.0, 30.0, 30.0]));
        // Touching counts as intersecting (closed).
        assert!(bbox_intersects(&outer, &[10.0, 10.0, 20.0, 20.0]));
    }
}
