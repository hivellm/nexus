//! External-id types for the storage layer.
//!
//! Re-exports [`ExternalId`] from the catalog and defines the
//! [`ConflictPolicy`] enum used by
//! [`RecordStore::create_node_with_external_id`] and
//! [`RecordStore::create_node_with_label_bits_and_external_id`].

/// Re-export so callers that go through the storage API only need this module.
pub use crate::catalog::external_id::ExternalId;

/// How to behave when the supplied external id already exists in the index.
///
/// Passed to [`super::RecordStore::create_node_with_external_id`] and the
/// parallel `label_bits` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictPolicy {
    /// Return [`Error::ExternalIdConflict`] — no record is written.
    ///
    /// This is the default.
    #[default]
    Error,

    /// Return the existing internal id — no new record is written and
    /// the supplied properties are discarded.
    Match,

    /// Reuse the existing internal id, overwrite properties through the
    /// property store, and leave label bits as-is.
    Replace,
}
