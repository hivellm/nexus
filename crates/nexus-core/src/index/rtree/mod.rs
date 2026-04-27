//! Packed Hilbert R-tree
//! (phase6_rtree-index-core).
//!
//! Replaces the grid-backed `crate::geospatial::rtree::RTreeIndex`
//! with a real R-tree:
//!
//! - **8 KB pages** mapped through `crate::page_cache`. Page layout
//!   (see [`page`]): 32 B header + up to 127 × 64 B [`ChildRef`]
//!   entries.
//! - **Bulk-load via Hilbert sort** (see [`hilbert`]) — packs entries
//!   in row-major Hilbert-curve order so spatially adjacent points
//!   share parent pages. Deterministic across replicas given the
//!   same input.
//! - **Incremental insert / delete** with quadratic split + leaf
//!   underflow re-insert (slice §3, follow-up).
//! - **Range, k-NN, withinDistance** queries with a priority-queue
//!   walk that terminates after `k` leaves are popped (slice §4).
//! - **WAL + MVCC** integration (slice §6) so spatial mutations
//!   replay deterministically after a crash.
//!
//! This module ships the on-disk page codec and the Hilbert bulk-
//! load primitive. The remaining slices land in follow-up commits
//! against this same module tree.

pub mod hilbert;
pub mod packer;
pub mod page;

pub use hilbert::{hilbert_index_2d, hilbert_index_3d};
pub use packer::{PACK_TARGET_FANOUT, PackedTree, bounding_box, bulk_pack};
pub use page::{ChildRef, PageDecodeError, RTreePageHeader, decode_page, encode_page};

/// Page size used by every R-tree page on disk and in memory.
/// Mirrors `crate::page_cache::PAGE_SIZE` so the same page cache
/// can be reused without per-index page-size accounting.
pub const RTREE_PAGE_SIZE: usize = 8192;

/// Minimum fanout enforced after a successful bulk-load. Pages
/// other than the rightmost sibling at every level SHALL contain at
/// least this many children — the spec calls for "no page below 64
/// children after bulk-load" so range / k-NN walks have predictable
/// fanout.
pub const RTREE_MIN_FANOUT: u16 = 64;

/// Maximum fanout per page. Cannot be exceeded by any operation —
/// inserts that would push a page past this number trigger the
/// quadratic split path (slice §3).
pub const RTREE_MAX_FANOUT: u16 = 127;

/// Magic value carried in the page header so a reader can fail fast
/// on a corrupt or wrong-type page. ASCII `"NXRT"` (Nexus R-Tree)
/// in big-endian.
pub const RTREE_PAGE_MAGIC: u32 = u32::from_be_bytes(*b"NXRT");

/// Page layout version. Bumped when [`encode_page`] /
/// [`decode_page`] make a backwards-incompatible change.
pub const RTREE_PAGE_VERSION: u16 = 1;
