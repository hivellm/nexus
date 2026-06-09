//! UDF, stored-procedure, property-index, and external-id extension methods
//! for [`Catalog`].
//!
//! These methods persist supplementary catalog data that is not part of the
//! core label/type/key mappings.

use crate::Result;
use crate::catalog::external_id::ExternalId;
use crate::catalog::external_id_index::ExternalIdIndex;
use crate::catalog::store::Catalog;

impl Catalog {
    // ── UDF storage ─────────────────────────────────────────────────────────

    /// Store a UDF signature in the catalog.
    pub fn store_udf(&self, signature: &crate::udf::UdfSignature) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.udf_db.put(&mut wtxn, &signature.name, signature)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Get a UDF signature from the catalog.
    pub fn get_udf(&self, name: &str) -> Result<Option<crate::udf::UdfSignature>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.udf_db.get(&rtxn, name)?)
    }

    /// List all UDF names stored in the catalog.
    pub fn list_udfs(&self) -> Result<Vec<String>> {
        let rtxn = self.env.read_txn()?;
        let iter = self.udf_db.iter(&rtxn)?;
        Ok(iter
            .filter_map(|r| r.ok())
            .map(|(name, _)| name.to_string())
            .collect())
    }

    /// Remove a UDF from the catalog.
    pub fn remove_udf(&self, name: &str) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.udf_db.delete(&mut wtxn, name)?;
        wtxn.commit()?;
        Ok(())
    }

    // ── Procedure storage ────────────────────────────────────────────────────

    /// Store a procedure signature in the catalog.
    pub fn store_procedure(
        &self,
        signature: &crate::graph::procedures::ProcedureSignature,
    ) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.procedure_db
            .put(&mut wtxn, &signature.name, signature)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Get a procedure signature from the catalog.
    pub fn get_procedure(
        &self,
        name: &str,
    ) -> Result<Option<crate::graph::procedures::ProcedureSignature>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.procedure_db.get(&rtxn, name)?)
    }

    /// List all procedure names stored in the catalog.
    pub fn list_procedures(&self) -> Result<Vec<String>> {
        let rtxn = self.env.read_txn()?;
        let iter = self.procedure_db.iter(&rtxn)?;
        Ok(iter
            .filter_map(|r| r.ok())
            .map(|(name, _)| name.to_string())
            .collect())
    }

    /// Remove a procedure from the catalog.
    pub fn remove_procedure(&self, name: &str) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.procedure_db.delete(&mut wtxn, name)?;
        wtxn.commit()?;
        Ok(())
    }

    // ── Property-index persistence ───────────────────────────────────────────

    /// Durably record that a property index exists on `(label_id, key_id)`
    /// so it can be rebuilt after a restart (issue #11). Idempotent.
    pub fn persist_property_index(&self, label_id: u32, key_id: u32) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.property_index_db
            .put(&mut wtxn, &(label_id, key_id), &())?;
        wtxn.commit()?;
        Ok(())
    }

    /// Remove a durable property-index definition (on `DROP INDEX`).
    pub fn remove_property_index(&self, label_id: u32, key_id: u32) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.property_index_db
            .delete(&mut wtxn, &(label_id, key_id))?;
        wtxn.commit()?;
        Ok(())
    }

    /// List every persisted property-index definition `(label_id, key_id)`.
    /// Used at startup to rebuild the typed property index.
    pub fn list_property_indexes(&self) -> Result<Vec<(u32, u32)>> {
        let rtxn = self.env.read_txn()?;
        let iter = self.property_index_db.iter(&rtxn)?;
        Ok(iter.filter_map(|r| r.ok()).map(|(k, _)| k).collect())
    }

    // ── External-id index ────────────────────────────────────────────────────

    /// Return a reference to the external-id index.
    ///
    /// Use this to call `put_if_absent`, `get_internal`, `get_external`,
    /// `delete`, and `iter`.  The index operates on caller-supplied
    /// transactions so it participates in the same atomicity domain as
    /// other catalog writes.
    pub fn external_id_index(&self) -> &ExternalIdIndex {
        &self.external_id_index
    }

    /// Verify that the external-id forward and reverse maps agree.
    ///
    /// In debug builds this is called automatically; in release builds
    /// callers can invoke it explicitly (e.g. from a `--verify` CLI path
    /// or during tests) to assert catalog integrity.
    pub fn verify_external_ids(&self) -> Result<()> {
        self.external_id_index.verify_consistency()
    }
}
