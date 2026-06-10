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
