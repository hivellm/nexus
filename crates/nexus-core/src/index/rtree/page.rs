//! On-disk page codec for the R-tree index
//! (phase6_rtree-index-core §1).
//!
//! ## Layout
//!
//! Every page is exactly [`super::RTREE_PAGE_SIZE`] (8 KB) bytes
//! and starts with a 32-byte header:
//!
//! ```text
//!   offset  size  field
//!   ------  ----  -------------------------------------------------
//!     0      4    magic       — [`super::RTREE_PAGE_MAGIC`]
//!     4      2    version     — [`super::RTREE_PAGE_VERSION`]
//!     6      1    level       — 0 = leaf, 1 = leaves' parent, …
//!     7      1    flags       — reserved (must be 0 in v1)
//!     8      2    count       — number of valid `ChildRef`s
//!    10      6    _reserved   — must be zero
//!    16     16    page_id     — owning page id (u128 little-endian)
//! ```
//!
//! Followed by `count` × 64-byte [`ChildRef`] entries:
//!
//! ```text
//!   offset  size  field
//!   ------  ----  -------------------------------------------------
//!     0     32    bbox        — [min_x, min_y, max_x, max_y] (f64 LE)
//!    32      8    child_ptr   — u64 LE; for leaves this is the
//!                                owning node_id, for inner pages
//!                                the child page id
//!    40      8    extra       — leaf: f64 z-coord (0.0 if 2-D);
//!                                inner: u64 child page level
//!    48     16    _pad        — zeroed
//! ```
//!
//! 32 B + 127 × 64 B = 32 + 8128 = 8160 < 8192. The remaining
//! 32 bytes are zero-padded so the on-disk image is reproducible
//! across runs.
//!
//! ## Determinism
//!
//! Encoding writes every header byte and every padding byte, so
//! two calls to [`encode_page`] with the same `header.count`
//! `entries` slice produce byte-identical output. This is the
//! property the bulk-load determinism test relies on.

use std::convert::TryFrom;

use thiserror::Error;

use super::{RTREE_MAX_FANOUT, RTREE_PAGE_MAGIC, RTREE_PAGE_SIZE, RTREE_PAGE_VERSION};

/// 32-byte page header. See module docs for layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RTreePageHeader {
    /// Tree level. 0 = leaf, 1 = parents of leaves, etc.
    pub level: u8,
    /// Reserved flag bits. Must be 0 in v1.
    pub flags: u8,
    /// Number of valid `ChildRef` entries that follow.
    pub count: u16,
    /// Owning page id (allocated by the page allocator).
    pub page_id: u128,
}

impl RTreePageHeader {
    /// Build a fresh header for `count` entries at `level`.
    pub fn new(level: u8, count: u16, page_id: u128) -> Self {
        Self {
            level,
            flags: 0,
            count,
            page_id,
        }
    }
}

/// 64-byte child reference. Used both at leaves (where `child_ptr`
/// is the owning `node_id` and `extra` is the optional z-coord)
/// and at inner pages (where `child_ptr` is the child page id and
/// `extra` is the child page level).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChildRef {
    /// `[min_x, min_y, max_x, max_y]` covering the child sub-tree.
    pub bbox: [f64; 4],
    /// Leaf: owning node id. Inner: child page id.
    pub child_ptr: u64,
    /// Leaf: z-coord (`0.0` for 2-D). Inner: child page level.
    pub extra: u64,
}

impl ChildRef {
    /// Construct a leaf entry covering `point` exactly. The bbox
    /// degenerates to `[x, y, x, y]` (all four coords equal) so the
    /// usual range-search predicate still works.
    pub fn leaf_point_2d(node_id: u64, x: f64, y: f64) -> Self {
        Self {
            bbox: [x, y, x, y],
            child_ptr: node_id,
            extra: 0,
        }
    }

    /// Construct a leaf entry for a 3-D point. The 2-D bbox degenerates
    /// to a single point on the (x, y) plane and `extra` carries the
    /// z-coord. 3-D within-distance queries combine the two.
    pub fn leaf_point_3d(node_id: u64, x: f64, y: f64, z: f64) -> Self {
        Self {
            bbox: [x, y, x, y],
            child_ptr: node_id,
            extra: z.to_bits(),
        }
    }

    /// Construct an inner-page entry pointing at child `page_id` at
    /// `child_level` covering `bbox`.
    pub fn inner(page_id: u64, child_level: u64, bbox: [f64; 4]) -> Self {
        Self {
            bbox,
            child_ptr: page_id,
            extra: child_level,
        }
    }

    /// `true` when `self.bbox` overlaps `other` (closed intervals).
    pub fn intersects(&self, other: &[f64; 4]) -> bool {
        self.bbox[0] <= other[2]
            && self.bbox[2] >= other[0]
            && self.bbox[1] <= other[3]
            && self.bbox[3] >= other[1]
    }
}

/// Errors surfaced by [`decode_page`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PageDecodeError {
    /// Buffer was not exactly [`RTREE_PAGE_SIZE`] bytes.
    #[error("R-tree page must be {expected} bytes, got {actual}")]
    InvalidLength { expected: usize, actual: usize },
    /// Magic field did not match [`RTREE_PAGE_MAGIC`].
    #[error("R-tree page magic mismatch (expected 0x{expected:08x}, got 0x{actual:08x})")]
    BadMagic { expected: u32, actual: u32 },
    /// Version field did not match [`RTREE_PAGE_VERSION`].
    #[error("R-tree page version unsupported (expected {expected}, got {actual})")]
    BadVersion { expected: u16, actual: u16 },
    /// `count` exceeds [`RTREE_MAX_FANOUT`].
    #[error("R-tree page fanout {actual} exceeds max {max}")]
    FanoutExceeded { actual: u16, max: u16 },
    /// Reserved bytes carried a non-zero value.
    #[error("R-tree page has non-zero reserved bytes")]
    NonZeroReserved,
}

/// Constants matching the layout documented at the top of the file.
const HEADER_SIZE: usize = 32;
const ENTRY_SIZE: usize = 64;

/// Encode `header` + `entries` into an 8 KB page buffer. The output
/// is exactly [`RTREE_PAGE_SIZE`] bytes long; trailing padding is
/// zeroed so two calls with the same inputs produce identical bytes.
///
/// # Panics
///
/// Panics if `entries.len() != header.count` or
/// `header.count > RTREE_MAX_FANOUT`. The encoder writes valid
/// pages only — callers are responsible for trimming or padding
/// before they get here.
pub fn encode_page(header: &RTreePageHeader, entries: &[ChildRef]) -> [u8; RTREE_PAGE_SIZE] {
    assert!(
        header.count as usize == entries.len(),
        "encode_page: header.count {} disagrees with entries.len() {}",
        header.count,
        entries.len(),
    );
    assert!(
        header.count <= RTREE_MAX_FANOUT,
        "encode_page: header.count {} > RTREE_MAX_FANOUT ({})",
        header.count,
        RTREE_MAX_FANOUT,
    );

    let mut buf = [0u8; RTREE_PAGE_SIZE];

    // Header
    buf[0..4].copy_from_slice(&RTREE_PAGE_MAGIC.to_be_bytes());
    buf[4..6].copy_from_slice(&RTREE_PAGE_VERSION.to_le_bytes());
    buf[6] = header.level;
    buf[7] = header.flags;
    buf[8..10].copy_from_slice(&header.count.to_le_bytes());
    // Bytes 10..16 are reserved zero (already zeroed by the [0u8; …] init).
    buf[16..32].copy_from_slice(&header.page_id.to_le_bytes());

    // Entries
    for (i, e) in entries.iter().enumerate() {
        let off = HEADER_SIZE + i * ENTRY_SIZE;
        for (j, v) in e.bbox.iter().enumerate() {
            buf[off + j * 8..off + (j + 1) * 8].copy_from_slice(&v.to_le_bytes());
        }
        buf[off + 32..off + 40].copy_from_slice(&e.child_ptr.to_le_bytes());
        buf[off + 40..off + 48].copy_from_slice(&e.extra.to_le_bytes());
        // off + 48 .. off + 64 stays zero-padded.
    }

    buf
}

/// Decode an 8 KB page buffer into its header + entries. Validates
/// magic, version, fanout, and reserved-bytes invariants.
pub fn decode_page(buf: &[u8]) -> Result<(RTreePageHeader, Vec<ChildRef>), PageDecodeError> {
    if buf.len() != RTREE_PAGE_SIZE {
        return Err(PageDecodeError::InvalidLength {
            expected: RTREE_PAGE_SIZE,
            actual: buf.len(),
        });
    }

    let magic = u32::from_be_bytes(buf[0..4].try_into().expect("4 bytes"));
    if magic != RTREE_PAGE_MAGIC {
        return Err(PageDecodeError::BadMagic {
            expected: RTREE_PAGE_MAGIC,
            actual: magic,
        });
    }

    let version = u16::from_le_bytes(buf[4..6].try_into().expect("2 bytes"));
    if version != RTREE_PAGE_VERSION {
        return Err(PageDecodeError::BadVersion {
            expected: RTREE_PAGE_VERSION,
            actual: version,
        });
    }

    let level = buf[6];
    let flags = buf[7];
    let count = u16::from_le_bytes(buf[8..10].try_into().expect("2 bytes"));
    if count > RTREE_MAX_FANOUT {
        return Err(PageDecodeError::FanoutExceeded {
            actual: count,
            max: RTREE_MAX_FANOUT,
        });
    }
    if buf[10..16].iter().any(|b| *b != 0) {
        return Err(PageDecodeError::NonZeroReserved);
    }

    let page_id = u128::from_le_bytes(buf[16..32].try_into().expect("16 bytes"));

    let header = RTreePageHeader {
        level,
        flags,
        count,
        page_id,
    };

    let mut entries = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let off = HEADER_SIZE + i * ENTRY_SIZE;
        let bbox = [
            f64::from_le_bytes(buf[off..off + 8].try_into().expect("8 bytes")),
            f64::from_le_bytes(buf[off + 8..off + 16].try_into().expect("8 bytes")),
            f64::from_le_bytes(buf[off + 16..off + 24].try_into().expect("8 bytes")),
            f64::from_le_bytes(buf[off + 24..off + 32].try_into().expect("8 bytes")),
        ];
        let child_ptr = u64::from_le_bytes(buf[off + 32..off + 40].try_into().expect("8 bytes"));
        let extra = u64::from_le_bytes(buf[off + 40..off + 48].try_into().expect("8 bytes"));
        entries.push(ChildRef {
            bbox,
            child_ptr,
            extra,
        });
    }

    Ok((header, entries))
}

/// Convert a decoded leaf entry's `extra` field to a z-coord f64.
/// Useful for callers building 3-D `Point`s out of decoded pages.
pub fn leaf_z_from_extra(entry: &ChildRef) -> f64 {
    f64::from_bits(entry.extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_capacity_entries() -> Vec<ChildRef> {
        (0..RTREE_MAX_FANOUT)
            .map(|i| ChildRef::leaf_point_2d(u64::from(i), f64::from(i), f64::from(i) * 2.0))
            .collect()
    }

    #[test]
    fn round_trip_empty_page() {
        let header = RTreePageHeader::new(0, 0, 42);
        let buf = encode_page(&header, &[]);
        let (decoded_h, entries) = decode_page(&buf).unwrap();
        assert_eq!(decoded_h, header);
        assert!(entries.is_empty());
    }

    #[test]
    fn round_trip_single_leaf_entry() {
        let header = RTreePageHeader::new(0, 1, 7);
        let entry = ChildRef::leaf_point_2d(99, 1.5, -2.25);
        let buf = encode_page(&header, std::slice::from_ref(&entry));
        let (decoded_h, entries) = decode_page(&buf).unwrap();
        assert_eq!(decoded_h, header);
        assert_eq!(entries, vec![entry]);
    }

    #[test]
    fn round_trip_full_capacity_page() {
        let entries = full_capacity_entries();
        let header = RTreePageHeader::new(0, RTREE_MAX_FANOUT, 11);
        let buf = encode_page(&header, &entries);
        let (decoded_h, decoded) = decode_page(&buf).unwrap();
        assert_eq!(decoded_h, header);
        assert_eq!(decoded, entries);
    }

    #[test]
    fn round_trip_inner_page_carries_child_levels() {
        let entries: Vec<_> = (0..8u64)
            .map(|i| ChildRef::inner(1000 + i, 1, [-1.0, -1.0, 1.0, 1.0]))
            .collect();
        let header = RTreePageHeader::new(2, u16::try_from(entries.len()).unwrap(), 1);
        let buf = encode_page(&header, &entries);
        let (decoded_h, decoded) = decode_page(&buf).unwrap();
        assert_eq!(decoded_h, header);
        assert_eq!(decoded, entries);
        for (orig, dec) in entries.iter().zip(decoded.iter()) {
            assert_eq!(dec.extra, orig.extra);
        }
    }

    #[test]
    fn leaf_3d_round_trips_z_coord_through_extra() {
        let entry = ChildRef::leaf_point_3d(5, 10.0, 20.0, 30.5);
        assert_eq!(leaf_z_from_extra(&entry), 30.5);
        let buf = encode_page(&RTreePageHeader::new(0, 1, 0), std::slice::from_ref(&entry));
        let (_, decoded) = decode_page(&buf).unwrap();
        assert_eq!(leaf_z_from_extra(&decoded[0]), 30.5);
    }

    #[test]
    fn encode_page_is_deterministic() {
        let entries = full_capacity_entries();
        let header = RTreePageHeader::new(0, RTREE_MAX_FANOUT, 3);
        let a = encode_page(&header, &entries);
        let b = encode_page(&header, &entries);
        assert_eq!(a, b, "encoder must be byte-identical across calls");
        // Trailing 32 bytes (after 32 + 127*64 = 8160) are zero.
        assert!(a[8160..].iter().all(|b| *b == 0));
    }

    #[test]
    fn decode_rejects_wrong_length() {
        let err = decode_page(&[0u8; RTREE_PAGE_SIZE - 1]).unwrap_err();
        assert!(matches!(err, PageDecodeError::InvalidLength { .. }));
    }

    #[test]
    fn decode_rejects_bad_magic() {
        let mut buf = encode_page(&RTreePageHeader::new(0, 0, 0), &[]);
        buf[0..4].copy_from_slice(&0xdeadbeef_u32.to_be_bytes());
        let err = decode_page(&buf).unwrap_err();
        assert!(matches!(err, PageDecodeError::BadMagic { .. }));
    }

    #[test]
    fn decode_rejects_bad_version() {
        let mut buf = encode_page(&RTreePageHeader::new(0, 0, 0), &[]);
        buf[4..6].copy_from_slice(&999_u16.to_le_bytes());
        let err = decode_page(&buf).unwrap_err();
        assert!(matches!(err, PageDecodeError::BadVersion { .. }));
    }

    #[test]
    fn decode_rejects_fanout_overflow() {
        let mut buf = encode_page(&RTreePageHeader::new(0, 0, 0), &[]);
        buf[8..10].copy_from_slice(&(RTREE_MAX_FANOUT + 1).to_le_bytes());
        let err = decode_page(&buf).unwrap_err();
        assert!(matches!(err, PageDecodeError::FanoutExceeded { .. }));
    }

    #[test]
    fn decode_rejects_non_zero_reserved() {
        let mut buf = encode_page(&RTreePageHeader::new(0, 0, 0), &[]);
        buf[12] = 1;
        let err = decode_page(&buf).unwrap_err();
        assert!(matches!(err, PageDecodeError::NonZeroReserved));
    }

    #[test]
    fn intersects_handles_touching_boxes() {
        let leaf = ChildRef::leaf_point_2d(1, 5.0, 5.0);
        // Same point → trivially intersects.
        assert!(leaf.intersects(&[5.0, 5.0, 5.0, 5.0]));
        // Surrounding bbox.
        assert!(leaf.intersects(&[0.0, 0.0, 10.0, 10.0]));
        // Disjoint.
        assert!(!leaf.intersects(&[6.0, 6.0, 7.0, 7.0]));
        // Edge-touch counts as intersecting (closed intervals).
        let inner = ChildRef::inner(0, 0, [0.0, 0.0, 5.0, 5.0]);
        assert!(inner.intersects(&[5.0, 0.0, 10.0, 5.0]));
    }
}
