use crate::error::Result;
use crate::storage::record_store::RecordStore;

impl RecordStore {
    /// Delete a node (mark as deleted).
    ///
    /// Does **not** clean up the external-id index.  Use
    /// [`RecordStore::delete_node_with_catalog`] when the node may carry an
    /// external id.
    pub fn delete_node(&mut self, node_id: u64) -> Result<()> {
        let mut record = self.read_node(node_id)?;
        record.mark_deleted();
        self.write_node(node_id, &record)
    }

    /// Delete a node and atomically remove its external-id index entries.
    ///
    /// Opens a write transaction on the catalog LMDB env, calls
    /// [`ExternalIdIndex::delete`] (a no-op when the node has no external id),
    /// then marks the node record as deleted — all within the same logical
    /// operation.  The catalog write transaction is committed before the
    /// in-memory record is updated, which is safe because the record store
    /// is single-writer.
    pub fn delete_node_with_catalog(
        &mut self,
        node_id: u64,
        catalog: &crate::catalog::Catalog,
    ) -> Result<()> {
        // Remove external-id mappings first (while the node is still "live").
        let mut wtxn = catalog.write_txn()?;
        catalog.external_id_index().delete(&mut wtxn, node_id)?;
        wtxn.commit()?;

        // Mark the record as deleted.
        let mut record = self.read_node(node_id)?;
        record.mark_deleted();
        self.write_node(node_id, &record)
    }

    /// Delete a relationship (mark as deleted)
    pub fn delete_rel(&mut self, rel_id: u64) -> Result<()> {
        let mut record = self.read_rel(rel_id)?;
        record.mark_deleted();
        self.write_rel(rel_id, &record)
    }
}
