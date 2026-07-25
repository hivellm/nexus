//! Metadata and statistics read/write methods for [`Catalog`].
//!
//! All methods in this module operate on the `metadata_db` and `stats_db`
//! LMDB sub-databases defined in [`crate::catalog::store`].

use crate::catalog::store::Catalog;
use crate::catalog::types::{CatalogMetadata, CatalogStats, LabelId, TypeId};
use crate::{Error, Result};

impl Catalog {
    // ── Metadata ────────────────────────────────────────────────────────────

    /// Get current metadata.
    pub fn get_metadata(&self) -> Result<CatalogMetadata> {
        let rtxn = self.env.read_txn()?;
        self.metadata_db
            .get(&rtxn, "main")?
            .ok_or_else(|| Error::Catalog("Metadata not found".into()))
    }

    /// Update metadata.
    pub fn update_metadata(&self, metadata: &CatalogMetadata) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.metadata_db.put(&mut wtxn, "main", metadata)?;
        wtxn.commit()?;
        Ok(())
    }

    // ── Statistics ──────────────────────────────────────────────────────────

    /// Get current statistics.
    pub fn get_statistics(&self) -> Result<CatalogStats> {
        let rtxn = self.env.read_txn()?;
        self.stats_db
            .get(&rtxn, "main")?
            .ok_or_else(|| Error::Catalog("Statistics not found".into()))
    }

    /// Update statistics.
    pub fn update_statistics(&self, stats: &CatalogStats) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.stats_db.put(&mut wtxn, "main", stats)?;
        wtxn.commit()?;
        Ok(())
    }

    // ── Node count helpers ──────────────────────────────────────────────────

    /// Increment node count for a label.
    pub fn increment_node_count(&self, label_id: LabelId) -> Result<()> {
        let mut stats = self.get_statistics()?;
        *stats.node_counts.entry(label_id).or_insert(0) += 1;
        self.update_statistics(&stats)
    }

    /// Phase 1 Optimization: Batch increment node counts (reduces I/O).
    /// Updates multiple label counts in a single transaction.
    pub fn batch_increment_node_counts(&self, updates: &[(LabelId, u32)]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut stats = self.get_statistics()?;
        for (label_id, count) in updates {
            *stats.node_counts.entry(*label_id).or_insert(0) += *count as u64;
        }
        self.update_statistics(&stats)
    }

    /// Phase 1 Optimization: Batch increment relationship counts (reduces I/O).
    /// Updates multiple type counts in a single transaction.
    pub fn batch_increment_rel_counts(&self, updates: &[(TypeId, u32)]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut stats = self.get_statistics()?;
        for (type_id, count) in updates {
            *stats.rel_counts.entry(*type_id).or_insert(0) += *count as u64;
        }
        self.update_statistics(&stats)
    }

    /// Decrement node count for a label.
    pub fn decrement_node_count(&self, label_id: LabelId) -> Result<()> {
        let mut stats = self.get_statistics()?;
        if let Some(count) = stats.node_counts.get_mut(&label_id) {
            *count = count.saturating_sub(1);
        }
        self.update_statistics(&stats)
    }

    // ── Relationship count helpers ──────────────────────────────────────────

    /// Increment relationship count for a type.
    pub fn increment_rel_count(&self, type_id: TypeId) -> Result<()> {
        let mut stats = self.get_statistics()?;
        *stats.rel_counts.entry(type_id).or_insert(0) += 1;
        self.update_statistics(&stats)
    }

    /// Decrement relationship count for a type.
    pub fn decrement_rel_count(&self, type_id: TypeId) -> Result<()> {
        let mut stats = self.get_statistics()?;
        if let Some(count) = stats.rel_counts.get_mut(&type_id) {
            *count = count.saturating_sub(1);
        }
        self.update_statistics(&stats)
    }

    // ── Aggregated counts ───────────────────────────────────────────────────

    /// Get total node count across all labels.
    /// This is used for optimizing `COUNT(*)` queries.
    pub fn get_total_node_count(&self) -> Result<u64> {
        let stats = self.get_statistics()?;
        Ok(stats.node_counts.values().sum())
    }

    /// Get total relationship count across all types.
    /// This is used for optimizing `COUNT(*)` queries on relationships.
    pub fn get_total_rel_count(&self) -> Result<u64> {
        let stats = self.get_statistics()?;
        Ok(stats.rel_counts.values().sum())
    }

    /// Get node count for a specific label.
    pub fn get_node_count(&self, label_id: LabelId) -> Result<u64> {
        let stats = self.get_statistics()?;
        Ok(*stats.node_counts.get(&label_id).unwrap_or(&0))
    }

    /// Get relationship count for a specific type.
    pub fn get_rel_count(&self, type_id: TypeId) -> Result<u64> {
        let stats = self.get_statistics()?;
        Ok(*stats.rel_counts.get(&type_id).unwrap_or(&0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::store::CATALOG_MMAP_INITIAL_SIZE;
    use crate::testing::TestContext;

    /// Create an isolated catalog for tests that need clean statistics state.
    /// Mirrors the `create_isolated_test_catalog` helper in `catalog::tests`.
    fn create_isolated_test_catalog() -> (Catalog, TestContext) {
        let ctx = TestContext::new();
        let catalog = Catalog::with_isolated_path(ctx.path(), CATALOG_MMAP_INITIAL_SIZE).unwrap();
        (catalog, ctx)
    }

    #[test]
    fn test_batch_increment_rel_counts_matches_sequential_increments() {
        // Catalog A: N sequential single-type increments across a few types.
        let (catalog_a, _dir_a) = create_isolated_test_catalog();
        for _ in 0..5 {
            catalog_a.increment_rel_count(0).unwrap();
        }
        for _ in 0..3 {
            catalog_a.increment_rel_count(1).unwrap();
        }
        catalog_a.increment_rel_count(2).unwrap();

        // Catalog B: one batched call covering the same totals.
        let (catalog_b, _dir_b) = create_isolated_test_catalog();
        catalog_b
            .batch_increment_rel_counts(&[(0, 5), (1, 3), (2, 1)])
            .unwrap();

        let stats_a = catalog_a.get_statistics().unwrap();
        let stats_b = catalog_b.get_statistics().unwrap();

        // Count-equivalence: batched totals must equal sequential totals.
        assert_eq!(stats_a.rel_counts, stats_b.rel_counts);
        assert_eq!(stats_b.rel_counts.get(&0), Some(&5));
        assert_eq!(stats_b.rel_counts.get(&1), Some(&3));
        assert_eq!(stats_b.rel_counts.get(&2), Some(&1));
    }

    #[test]
    fn test_batch_increment_rel_counts_empty_is_noop() {
        let (catalog, _dir) = create_isolated_test_catalog();
        catalog.increment_rel_count(0).unwrap();

        let before = catalog.get_statistics().unwrap();
        catalog.batch_increment_rel_counts(&[]).unwrap();
        let after = catalog.get_statistics().unwrap();

        assert_eq!(before.rel_counts, after.rel_counts);
    }
}
