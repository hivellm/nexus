//! LMDB-backed index mapping external node identifiers to internal `u64` ids.
//!
//! Two sub-databases are maintained:
//! - `external_ids`: `ExternalId` bytes → `u64` internal id (LE)
//! - `internal_ids`: `u64` internal id (LE) → `ExternalId` bytes
//!
//! Both are kept in sync in the same write transaction so the forward and
//! reverse maps are always consistent.

use heed::types::Bytes as HeedBytes;
use heed::{Database, Env, RoTxn, RwTxn};

use crate::catalog::external_id::{ExternalId, ExternalIdError};
use crate::{Error, Result};

// ──────────────────────────────────────────────────────────────────────────────
// Sub-database names
// ──────────────────────────────────────────────────────────────────────────────

/// LMDB sub-database name for the forward map (ExternalId bytes → internal id).
pub const EXTERNAL_IDS_DB: &str = "external_ids";
/// LMDB sub-database name for the reverse map (internal id → ExternalId bytes).
pub const INTERNAL_IDS_DB: &str = "internal_ids";

// ──────────────────────────────────────────────────────────────────────────────
// Error bridging
// ──────────────────────────────────────────────────────────────────────────────

impl From<ExternalIdError> for Error {
    fn from(e: ExternalIdError) -> Self {
        Error::Catalog(format!("external id error: {e}"))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// ExternalIdIndex
// ──────────────────────────────────────────────────────────────────────────────

/// Bidirectional LMDB index for external ↔ internal node id lookups.
///
/// Both the forward (`external_ids`) and reverse (`internal_ids`) sub-databases
/// are owned by this struct.  All mutations update both maps inside the
/// **same** write transaction supplied by the caller so the pair is always
/// atomically consistent.
pub struct ExternalIdIndex {
    /// Forward map: `ExternalId` bytes → internal id (8 LE bytes).
    forward: Database<HeedBytes, HeedBytes>,
    /// Reverse map: internal id (8 LE bytes) → `ExternalId` bytes.
    reverse: Database<HeedBytes, HeedBytes>,
    /// Handle to the environment — kept for integrity-check helper.
    env: Env,
}

impl ExternalIdIndex {
    /// Open (or create) both sub-databases inside the supplied write
    /// transaction.  Callers must commit `wtxn` after this returns.
    pub fn open(env: &Env, wtxn: &mut RwTxn<'_>) -> Result<Self> {
        let forward: Database<HeedBytes, HeedBytes> =
            env.create_database(wtxn, Some(EXTERNAL_IDS_DB))?;
        let reverse: Database<HeedBytes, HeedBytes> =
            env.create_database(wtxn, Some(INTERNAL_IDS_DB))?;
        Ok(Self {
            forward,
            reverse,
            env: env.clone(),
        })
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn id_to_le_bytes(id: u64) -> [u8; 8] {
        id.to_le_bytes()
    }

    fn le_bytes_to_id(b: &[u8]) -> Result<u64> {
        if b.len() != 8 {
            return Err(Error::Catalog(format!(
                "internal id must be 8 bytes, got {}",
                b.len()
            )));
        }
        let mut arr = [0u8; 8];
        arr.copy_from_slice(b);
        Ok(u64::from_le_bytes(arr))
    }

    // ── Write operations ──────────────────────────────────────────────────────

    /// Insert the mapping `ext → internal` if it does not already exist.
    ///
    /// Returns `None` on successful insert, or `Some(existing_id)` when
    /// the external id already maps to a different internal id.  Caller
    /// must decide how to handle the conflict — this method never
    /// overwrites.
    pub fn put_if_absent(
        &self,
        txn: &mut RwTxn<'_>,
        ext: &ExternalId,
        internal: u64,
    ) -> Result<Option<u64>> {
        let key = ext.to_bytes();
        // Check for existing entry.
        if let Some(existing_bytes) = self.forward.get(txn, key.as_slice())? {
            let existing_id = Self::le_bytes_to_id(existing_bytes)?;
            return Ok(Some(existing_id));
        }
        // Write forward.
        let id_bytes = Self::id_to_le_bytes(internal);
        self.forward.put(txn, key.as_slice(), id_bytes.as_slice())?;
        // Write reverse.
        self.reverse.put(txn, id_bytes.as_slice(), key.as_slice())?;
        Ok(None)
    }

    /// Delete the mappings for `internal` id (looks up the forward key via
    /// the reverse map).  Returns `true` if an entry was removed, `false`
    /// if no entry existed for that internal id.
    pub fn delete(&self, txn: &mut RwTxn<'_>, internal: u64) -> Result<bool> {
        let id_bytes = Self::id_to_le_bytes(internal);
        // Look up the external key from the reverse map.
        let ext_bytes_opt = self
            .reverse
            .get(txn, id_bytes.as_slice())?
            .map(|b| b.to_vec());
        let Some(ext_bytes) = ext_bytes_opt else {
            return Ok(false);
        };
        // Delete reverse first (while we still hold the value).
        self.reverse.delete(txn, id_bytes.as_slice())?;
        // Delete forward.
        self.forward.delete(txn, ext_bytes.as_slice())?;
        Ok(true)
    }

    // ── Read operations ───────────────────────────────────────────────────────

    /// Look up the internal id for an external id.
    pub fn get_internal(&self, txn: &RoTxn<'_>, ext: &ExternalId) -> Result<Option<u64>> {
        let key = ext.to_bytes();
        match self.forward.get(txn, key.as_slice())? {
            Some(b) => Ok(Some(Self::le_bytes_to_id(b)?)),
            None => Ok(None),
        }
    }

    /// Look up the external id for an internal id.
    pub fn get_external(&self, txn: &RoTxn<'_>, internal: u64) -> Result<Option<ExternalId>> {
        let id_bytes = Self::id_to_le_bytes(internal);
        match self.reverse.get(txn, id_bytes.as_slice())? {
            Some(b) => {
                let ext = ExternalId::from_bytes(b)?;
                Ok(Some(ext))
            }
            None => Ok(None),
        }
    }

    /// Iterate all `(ExternalId, internal_id)` pairs in the forward map.
    pub fn iter<'txn>(&self, txn: &'txn RoTxn<'_>) -> Result<ExternalIdIter<'txn>> {
        let raw = self.forward.iter(txn)?;
        Ok(ExternalIdIter { raw })
    }

    // ── Integrity check ───────────────────────────────────────────────────────

    /// Verify that the forward and reverse maps are fully consistent.
    ///
    /// Returns `Ok(())` if every `(ext → id)` entry in the forward map has a
    /// matching `(id → ext)` in the reverse map and the cardinalities agree.
    /// Returns `Err` with a description of the first inconsistency found.
    pub fn verify_consistency(&self) -> Result<()> {
        let rtxn = self.env.read_txn()?;
        let forward_count = self.forward.len(&rtxn)?;
        let reverse_count = self.reverse.len(&rtxn)?;

        if forward_count != reverse_count {
            return Err(Error::Catalog(format!(
                "external-id index consistency error: forward has {forward_count} entries, \
                 reverse has {reverse_count}"
            )));
        }

        // Verify every forward entry has a matching reverse entry.
        for item in self.forward.iter(&rtxn)? {
            let (ext_bytes, id_bytes) = item?;
            let internal = Self::le_bytes_to_id(id_bytes)?;
            let rev_key = Self::id_to_le_bytes(internal);
            match self.reverse.get(&rtxn, rev_key.as_slice())? {
                None => {
                    return Err(Error::Catalog(format!(
                        "external-id reverse map missing entry for internal id {internal}"
                    )));
                }
                Some(stored_ext) => {
                    if stored_ext != ext_bytes {
                        return Err(Error::Catalog(format!(
                            "external-id mismatch for internal id {internal}: \
                             forward key {ext_bytes:?} != reverse value {stored_ext:?}"
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Iterator
// ──────────────────────────────────────────────────────────────────────────────

/// Iterator over `(ExternalId, internal_id)` pairs.
pub struct ExternalIdIter<'txn> {
    raw: heed::RoIter<'txn, HeedBytes, HeedBytes>,
}

impl Iterator for ExternalIdIter<'_> {
    type Item = Result<(ExternalId, u64)>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.raw.next()?;
        Some(item.map_err(Error::from).and_then(|(k, v)| {
            let ext = ExternalId::from_bytes(k)?;
            let id = ExternalIdIndex::le_bytes_to_id(v)?;
            Ok((ext, id))
        }))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CATALOG_MMAP_INITIAL_SIZE;
    use crate::catalog::external_id::HashKind;
    use heed::EnvOpenOptions;
    use std::sync::Arc;

    fn make_env() -> Arc<heed::Env> {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: standard env open, single-process, no concurrent writers from other processes
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(CATALOG_MMAP_INITIAL_SIZE)
                .max_dbs(4)
                .max_readers(32)
                .open(dir.into_path())
                .unwrap()
        };
        Arc::new(env)
    }

    fn open_index(env: &heed::Env) -> ExternalIdIndex {
        let mut wtxn = env.write_txn().unwrap();
        let idx = ExternalIdIndex::open(env, &mut wtxn).unwrap();
        wtxn.commit().unwrap();
        idx
    }

    // ── Insert and lookup ─────────────────────────────────────────────────────

    #[test]
    fn test_insert_and_lookup_forward() {
        let env = make_env();
        let idx = open_index(&env);

        let ext = ExternalId::try_str("doc:001".to_string()).unwrap();
        let internal = 42u64;

        let mut wtxn = env.write_txn().unwrap();
        let conflict = idx.put_if_absent(&mut wtxn, &ext, internal).unwrap();
        wtxn.commit().unwrap();

        assert_eq!(conflict, None, "first insert should have no conflict");

        let rtxn = env.read_txn().unwrap();
        let found = idx.get_internal(&rtxn, &ext).unwrap();
        assert_eq!(found, Some(internal));
    }

    #[test]
    fn test_insert_and_lookup_reverse() {
        let env = make_env();
        let idx = open_index(&env);

        let ext = ExternalId::try_hash(HashKind::Blake3, vec![0xABu8; 32]).unwrap();
        let internal = 99u64;

        let mut wtxn = env.write_txn().unwrap();
        idx.put_if_absent(&mut wtxn, &ext, internal).unwrap();
        wtxn.commit().unwrap();

        let rtxn = env.read_txn().unwrap();
        let found = idx.get_external(&rtxn, internal).unwrap();
        assert_eq!(found, Some(ext));
    }

    // ── Duplicate rejection ───────────────────────────────────────────────────

    #[test]
    fn test_put_if_absent_returns_existing_on_conflict() {
        let env = make_env();
        let idx = open_index(&env);

        let ext = ExternalId::try_uuid([1u8; 16]).unwrap();

        let mut wtxn = env.write_txn().unwrap();
        let first = idx.put_if_absent(&mut wtxn, &ext, 10).unwrap();
        wtxn.commit().unwrap();

        assert_eq!(first, None);

        // Try inserting the same external id with a different internal id.
        let mut wtxn2 = env.write_txn().unwrap();
        let second = idx.put_if_absent(&mut wtxn2, &ext, 20).unwrap();
        wtxn2.commit().unwrap();

        assert_eq!(second, Some(10), "should return existing internal id");
    }

    // ── Delete ────────────────────────────────────────────────────────────────

    #[test]
    fn test_delete_removes_both_directions() {
        let env = make_env();
        let idx = open_index(&env);

        let ext = ExternalId::try_str("to-delete".to_string()).unwrap();
        let internal = 7u64;

        let mut wtxn = env.write_txn().unwrap();
        idx.put_if_absent(&mut wtxn, &ext, internal).unwrap();
        wtxn.commit().unwrap();

        let mut wtxn2 = env.write_txn().unwrap();
        let deleted = idx.delete(&mut wtxn2, internal).unwrap();
        wtxn2.commit().unwrap();

        assert!(deleted, "delete should return true");

        let rtxn = env.read_txn().unwrap();
        assert_eq!(idx.get_internal(&rtxn, &ext).unwrap(), None);
        assert_eq!(idx.get_external(&rtxn, internal).unwrap(), None);
    }

    #[test]
    fn test_delete_nonexistent_returns_false() {
        let env = make_env();
        let idx = open_index(&env);

        let mut wtxn = env.write_txn().unwrap();
        let deleted = idx.delete(&mut wtxn, 999).unwrap();
        wtxn.commit().unwrap();

        assert!(!deleted);
    }

    // ── Reopen and reload ─────────────────────────────────────────────────────

    #[test]
    fn test_reopen_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.into_path();

        let ext = ExternalId::try_str("persistent-key".to_string()).unwrap();
        let internal = 55u64;

        // First open: insert.
        {
            // SAFETY: standard env open
            let env = unsafe {
                EnvOpenOptions::new()
                    .map_size(CATALOG_MMAP_INITIAL_SIZE)
                    .max_dbs(4)
                    .max_readers(32)
                    .open(&path)
                    .unwrap()
            };
            let idx = open_index(&env);
            let mut wtxn = env.write_txn().unwrap();
            idx.put_if_absent(&mut wtxn, &ext, internal).unwrap();
            wtxn.commit().unwrap();
            env.force_sync().unwrap();
        }

        // Second open: verify data survived.
        {
            // SAFETY: standard env open
            let env = unsafe {
                EnvOpenOptions::new()
                    .map_size(CATALOG_MMAP_INITIAL_SIZE)
                    .max_dbs(4)
                    .max_readers(32)
                    .open(&path)
                    .unwrap()
            };
            let idx = open_index(&env);
            let rtxn = env.read_txn().unwrap();
            assert_eq!(idx.get_internal(&rtxn, &ext).unwrap(), Some(internal));
            assert_eq!(idx.get_external(&rtxn, internal).unwrap(), Some(ext));
        }
    }

    // ── Forward/reverse consistency ───────────────────────────────────────────

    #[test]
    fn test_consistency_check_over_populated_set() {
        let env = make_env();
        let idx = open_index(&env);

        let entries: Vec<(ExternalId, u64)> = (0..10u64)
            .map(|i| (ExternalId::try_str(format!("key-{i}")).unwrap(), i * 100))
            .collect();

        let mut wtxn = env.write_txn().unwrap();
        for (ext, id) in &entries {
            idx.put_if_absent(&mut wtxn, ext, *id).unwrap();
        }
        wtxn.commit().unwrap();

        idx.verify_consistency().unwrap();
    }

    // ── Iterator ─────────────────────────────────────────────────────────────

    #[test]
    fn test_iter_returns_all_entries() {
        let env = make_env();
        let idx = open_index(&env);

        let mut wtxn = env.write_txn().unwrap();
        for i in 0u64..5 {
            let ext = ExternalId::try_str(format!("entry-{i}")).unwrap();
            idx.put_if_absent(&mut wtxn, &ext, i).unwrap();
        }
        wtxn.commit().unwrap();

        let rtxn = env.read_txn().unwrap();
        let all: Vec<_> = idx
            .iter(&rtxn)
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(all.len(), 5);
    }
}
