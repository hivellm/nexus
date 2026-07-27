//! `impl Default for Executor` — builds an isolated, temp-dir-backed
//! executor used by tests and benchmarks.

use super::super::*;
use crate::catalog::Catalog;
use crate::index::{KnnIndex, LabelIndex};
use crate::storage::RecordStore;

impl Default for Executor {
    /// Build a fresh `Executor` backed by an isolated record store
    /// rooted at a throwaway temp directory.
    ///
    /// Every call allocates its own `RecordStore`, `Catalog`,
    /// `LabelIndex`, and `KnnIndex`, so concurrent tests cannot see
    /// each other's nodes / relationships. The temp directory holding
    /// the record store is created via [`RecordStore::new_temporary`],
    /// which attaches a reference-counted cleanup guard to the store:
    /// the directory is removed once the last clone of the store (and
    /// therefore the last live memory-mapped handle) is dropped, so
    /// concurrent readers of the same `Executor` clone keep working
    /// exactly as before, but the directory no longer accumulates on
    /// disk once every clone goes away.
    ///
    /// Historically this function returned a `RecordStore` clone drawn
    /// from a single process-wide shared store. Every caller observed
    /// the same store, so any test that created nodes polluted every
    /// other test. The current implementation gives each caller its own
    /// isolated state — tests that previously relied (even
    /// accidentally) on cross-test state will need to be updated.
    fn default() -> Self {
        let store = RecordStore::new_temporary().expect("Failed to create temporary record store");
        let catalog = Catalog::default();
        let label_index = LabelIndex::default();
        let knn_index = KnnIndex::new_default(crate::index::DEFAULT_VECTORIZER_DIMENSION)
            .expect("Failed to create default KNN index");

        Self::new(&catalog, &store, &label_index, &knn_index)
            .expect("Failed to create default executor")
    }
}
