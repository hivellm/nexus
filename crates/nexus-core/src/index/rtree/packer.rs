//! Bottom-up bulk packer for the Hilbert R-tree
//! (phase6_rtree-index-core §2.3 + §2.4).
//!
//! Takes a Hilbert-sorted leaf entry stream (see
//! [`super::hilbert::sort_by_hilbert_2d`]) and packs it into a
//! complete tree of [`super::page`] pages — leaves first, then
//! their parents level-by-level, until a single root remains.
//!
//! ## Algorithm
//!
//! 1. Group the leaf entries into chunks of [`PACK_TARGET_FANOUT`]
//!    (127). The final chunk gets the remainder.
//! 2. For each chunk, allocate a page id (monotonically
//!    increasing from 1; root is the last one), build an
//!    [`RTreePageHeader`] at level 0, encode an 8 KB page.
//! 3. Walk the just-emitted leaf-page list and synthesise an
//!    inner-level entry per page: bbox = union of the leaf's
//!    bounding boxes, `child_ptr` = page id, `extra` = child level
//!    (0 for leaves' parents).
//! 4. Repeat the chunk → page → parent-entry process at the next
//!    level. Stop when the level produces exactly one page —
//!    that's the root.
//!
//! ## Determinism
//!
//! Page ids are assigned in pack order (level 0 left-to-right,
//! then level 1, …, root last). The encoder itself is byte-
//! deterministic (see `page.rs`). Two runs with the same input
//! produce byte-identical pages and the same root id.

use super::page::{ChildRef, RTreePageHeader, encode_page};
use super::{RTREE_MAX_FANOUT, RTREE_PAGE_SIZE};

/// Maximum entries packed per page during bulk-load. Mirrors the
/// hard fanout cap; the packer never approaches the cap unless the
/// remainder branch shapes a final partial page.
pub const PACK_TARGET_FANOUT: usize = RTREE_MAX_FANOUT as usize;

/// Output of [`bulk_pack`]: every encoded page in the order they
/// were produced + the root page id.
#[derive(Debug, Clone)]
pub struct PackedTree {
    /// Encoded pages in pack order. Each entry is exactly
    /// [`RTREE_PAGE_SIZE`] bytes; concatenating them yields the
    /// on-disk image. Index `root_page_id - 1` (0-based) is the
    /// root because page ids are 1-based.
    pub pages: Vec<[u8; RTREE_PAGE_SIZE]>,
    /// Page id of the root page. `0` when the input was empty.
    pub root_page_id: u64,
    /// Number of levels in the packed tree (1 = leaf-only,
    /// 2 = one level of inner pages, …). `0` when empty.
    pub height: u8,
}

/// Pack a Hilbert-sorted slice of leaf [`ChildRef`] entries into a
/// complete R-tree. The caller is responsible for sorting the
/// entries first (e.g. via
/// [`super::hilbert::sort_by_hilbert_2d`]); this routine packs
/// them in the order received.
///
/// Returns an empty [`PackedTree`] (no pages, root id 0) when
/// `leaves` is empty. The caller decides whether that's an error
/// at their level.
pub fn bulk_pack(leaves: &[ChildRef]) -> PackedTree {
    if leaves.is_empty() {
        return PackedTree {
            pages: Vec::new(),
            root_page_id: 0,
            height: 0,
        };
    }

    let mut pages: Vec<[u8; RTREE_PAGE_SIZE]> = Vec::new();
    let mut next_page_id: u64 = 1;
    let mut current_level: u8 = 0;

    // Step 1: pack the leaf level.
    let mut current_entries: Vec<ChildRef> = Vec::with_capacity(leaves.len());
    for chunk in leaves.chunks(PACK_TARGET_FANOUT) {
        let page_id = next_page_id;
        next_page_id += 1;

        let header = RTreePageHeader::new(current_level, chunk.len() as u16, u128::from(page_id));
        pages.push(encode_page(&header, chunk));

        current_entries.push(ChildRef::inner(
            page_id,
            u64::from(current_level),
            bounding_box(chunk),
        ));
    }
    current_level += 1;

    // Step 2: collapse upwards. Stop once `current_entries` has
    // exactly one element — that's the root, already represented
    // by the most recently allocated page id.
    while current_entries.len() > 1 {
        let mut next_entries: Vec<ChildRef> =
            Vec::with_capacity(current_entries.len().div_ceil(PACK_TARGET_FANOUT));
        for chunk in current_entries.chunks(PACK_TARGET_FANOUT) {
            let page_id = next_page_id;
            next_page_id += 1;

            let header =
                RTreePageHeader::new(current_level, chunk.len() as u16, u128::from(page_id));
            pages.push(encode_page(&header, chunk));

            next_entries.push(ChildRef::inner(
                page_id,
                u64::from(current_level),
                bounding_box(chunk),
            ));
        }
        current_entries = next_entries;
        current_level += 1;
    }

    // The root id is the most recently allocated page id; height
    // is the number of levels we produced.
    let root_page_id = next_page_id - 1;
    PackedTree {
        pages,
        root_page_id,
        height: current_level,
    }
}

/// Compute the 2-D bounding box covering every entry in `entries`.
/// Used both for leaf → parent and parent → grandparent
/// promotions during packing.
///
/// # Panics
///
/// Panics if `entries` is empty. The packer never calls this with
/// an empty slice — every chunk produced by `chunks(...)` carries
/// at least one element.
pub fn bounding_box(entries: &[ChildRef]) -> [f64; 4] {
    assert!(
        !entries.is_empty(),
        "bounding_box called on an empty slice (pack invariant violated)"
    );
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for e in entries {
        if e.bbox[0] < min_x {
            min_x = e.bbox[0];
        }
        if e.bbox[1] < min_y {
            min_y = e.bbox[1];
        }
        if e.bbox[2] > max_x {
            max_x = e.bbox[2];
        }
        if e.bbox[3] > max_y {
            max_y = e.bbox[3];
        }
    }
    [min_x, min_y, max_x, max_y]
}

#[cfg(test)]
mod tests {
    use super::super::page::{decode_page, leaf_z_from_extra};
    use super::*;

    fn leaf(node_id: u64, x: f64, y: f64) -> ChildRef {
        ChildRef::leaf_point_2d(node_id, x, y)
    }

    #[test]
    fn empty_input_packs_nothing() {
        let out = bulk_pack(&[]);
        assert_eq!(out.pages.len(), 0);
        assert_eq!(out.root_page_id, 0);
        assert_eq!(out.height, 0);
    }

    #[test]
    fn single_leaf_packs_into_a_one_page_tree() {
        let entries = vec![leaf(42, 1.0, 2.0)];
        let out = bulk_pack(&entries);
        assert_eq!(out.pages.len(), 1);
        assert_eq!(out.root_page_id, 1);
        assert_eq!(out.height, 1);

        let (header, decoded) = decode_page(&out.pages[0]).unwrap();
        assert_eq!(header.level, 0);
        assert_eq!(header.count, 1);
        assert_eq!(header.page_id, 1);
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].child_ptr, 42);
    }

    #[test]
    fn fits_in_one_leaf_when_below_fanout() {
        let entries: Vec<_> = (0..50u64)
            .map(|i| leaf(i, f64::from(i as i32), 0.0))
            .collect();
        let out = bulk_pack(&entries);
        assert_eq!(out.pages.len(), 1);
        assert_eq!(out.height, 1);
        let (header, decoded) = decode_page(&out.pages[0]).unwrap();
        assert_eq!(header.count, 50);
        assert_eq!(decoded.len(), 50);
    }

    #[test]
    fn full_capacity_leaf_does_not_promote_to_inner_level() {
        // Exactly RTREE_MAX_FANOUT entries fit into a single leaf,
        // so the tree height stays at 1 and there's no inner page.
        let entries: Vec<_> = (0..PACK_TARGET_FANOUT as u64)
            .map(|i| leaf(i, f64::from(i as i32), f64::from(i as i32)))
            .collect();
        let out = bulk_pack(&entries);
        assert_eq!(out.pages.len(), 1);
        assert_eq!(out.height, 1);
        assert_eq!(out.root_page_id, 1);
    }

    #[test]
    fn overflow_promotes_to_two_levels() {
        // 200 leaves → ceil(200/127) = 2 leaf pages → 1 inner root.
        let entries: Vec<_> = (0..200u64)
            .map(|i| leaf(i, f64::from(i as i32), f64::from(i as i32) * 0.5))
            .collect();
        let out = bulk_pack(&entries);
        assert_eq!(out.pages.len(), 3, "two leaves + one root");
        assert_eq!(out.height, 2);
        assert_eq!(out.root_page_id, 3);

        // Root page is the last one packed; it points at both leaves.
        let (root_header, root_entries) = decode_page(&out.pages[2]).unwrap();
        assert_eq!(root_header.level, 1);
        assert_eq!(root_header.count, 2);
        assert!(root_entries.iter().any(|e| e.child_ptr == 1));
        assert!(root_entries.iter().any(|e| e.child_ptr == 2));
    }

    #[test]
    fn three_level_tree_for_more_than_fanout_squared() {
        // 127^2 = 16_129. Bumping a few past that forces a third
        // level. Pick something modest so the test stays fast.
        let n: u64 = (PACK_TARGET_FANOUT * PACK_TARGET_FANOUT + 1) as u64;
        let entries: Vec<_> = (0..n)
            .map(|i| leaf(i, f64::from(i as u32), f64::from(i as u32)))
            .collect();
        let out = bulk_pack(&entries);
        assert_eq!(out.height, 3);

        // Last page is the root.
        let root_idx = (out.root_page_id - 1) as usize;
        let (root_header, _) = decode_page(&out.pages[root_idx]).unwrap();
        assert_eq!(root_header.level, 2);
    }

    #[test]
    fn parent_bbox_unions_every_child_box() {
        // Manually crafted leaves so we know the union of their
        // boxes. After packing we re-decode the root and check
        // it covers all of them.
        let mut entries = vec![leaf(1, 0.0, 0.0), leaf(2, 10.0, 5.0), leaf(3, -2.0, 7.0)];
        let n = PACK_TARGET_FANOUT as u64;
        // Pad so the leaves split across at least two pages —
        // PACK_TARGET_FANOUT + a few extras forces an inner level.
        let pad_until = n + 5;
        for i in 4..=pad_until {
            entries.push(leaf(i, f64::from(i as u32), f64::from(i as u32)));
        }
        let out = bulk_pack(&entries);
        // Find any inner-level page (level >= 1) and verify the
        // union of its child bboxes covers every leaf point.
        let inner_pages: Vec<_> = out
            .pages
            .iter()
            .filter_map(|p| {
                let (h, e) = decode_page(p).ok()?;
                (h.level >= 1).then_some(e)
            })
            .collect();
        assert!(!inner_pages.is_empty(), "must have at least one inner page");
        let any = &inner_pages[0];
        // The union must enclose the originals.
        let bb = bounding_box(any);
        // -2.0 .. pad_until is the extreme x range across all leaves.
        assert!(bb[0] <= -2.0 + 1e-9);
        assert!(bb[2] >= f64::from(pad_until as u32));
    }

    #[test]
    fn pack_is_byte_identical_across_runs() {
        // §2.4 — byte-identical-output across replicas. We don't
        // need two physical machines for that property; the
        // encoder is deterministic so the same input must produce
        // the same bytes every time. Drift in *any* layer (sort
        // stability, page-id allocation, padding) would break
        // this.
        let make = || -> Vec<ChildRef> {
            (0..500u64)
                .map(|i| {
                    leaf(
                        i,
                        ((i.wrapping_mul(2654435761) >> 16) & 0xff) as f64,
                        ((i.wrapping_mul(40503) >> 8) & 0xff) as f64,
                    )
                })
                .collect()
        };
        let a = bulk_pack(&make());
        let b = bulk_pack(&make());
        assert_eq!(a.pages.len(), b.pages.len());
        for (pa, pb) in a.pages.iter().zip(b.pages.iter()) {
            assert_eq!(pa, pb);
        }
        assert_eq!(a.root_page_id, b.root_page_id);
        assert_eq!(a.height, b.height);
    }

    #[test]
    fn leaf_3d_z_coords_round_trip_through_pack() {
        let entries: Vec<_> = (0..3u64)
            .map(|i| {
                ChildRef::leaf_point_3d(i, f64::from(i as i32), 0.0, 1.5 + f64::from(i as i32))
            })
            .collect();
        let out = bulk_pack(&entries);
        let (header, decoded) = decode_page(&out.pages[0]).unwrap();
        assert_eq!(header.count, 3);
        for (i, d) in decoded.iter().enumerate() {
            assert_eq!(leaf_z_from_extra(d), 1.5 + i as f64);
        }
    }
}
