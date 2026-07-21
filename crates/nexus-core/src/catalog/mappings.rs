//! Label / type / key name ↔ ID bidirectional mapping methods for [`Catalog`].
//!
//! Every allocation uses an LMDB-backed counter derived atomically inside the
//! write transaction so multiple `Catalog` instances sharing one environment
//! never hand out the same ID to different names.

use crate::catalog::store::Catalog;
use crate::catalog::types::{KeyId, LabelId, TypeId};
use crate::{Error, Result};
use parking_lot::RwLock;

impl Catalog {
    // ── Internal ID allocators ──────────────────────────────────────────────

    /// Allocate the next label id from committed LMDB state inside an open
    /// write txn. Must be called while holding the env write txn so the scan
    /// is atomic w.r.t. other writers (LMDB serialises writers across Catalog
    /// instances and processes). Keeps the in-memory counter monotonic.
    fn alloc_label_id(&self, wtxn: &heed::RwTxn<'_>) -> Result<LabelId> {
        let new_id = self
            .label_name_to_id
            .iter(wtxn)?
            .map(|r| r.map(|(_, id)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        let mut next = self.next_label_id.write();
        if *next <= new_id {
            *next = new_id + 1;
        }
        Ok(new_id)
    }

    /// See [`alloc_label_id`]. Same atomic allocation for relationship types.
    fn alloc_type_id(&self, wtxn: &heed::RwTxn<'_>) -> Result<TypeId> {
        let new_id = self
            .type_name_to_id
            .iter(wtxn)?
            .map(|r| r.map(|(_, id)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        let mut next = self.next_type_id.write();
        if *next <= new_id {
            *next = new_id + 1;
        }
        Ok(new_id)
    }

    /// See [`alloc_label_id`]. Same atomic allocation for property keys.
    fn alloc_key_id(&self, wtxn: &heed::RwTxn<'_>) -> Result<KeyId> {
        let new_id = self
            .key_name_to_id
            .iter(wtxn)?
            .map(|r| r.map(|(_, id)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        let mut next = self.next_key_id.write();
        if *next <= new_id {
            *next = new_id + 1;
        }
        Ok(new_id)
    }

    // ── Label methods ───────────────────────────────────────────────────────

    /// Get or create a label ID.
    ///
    /// Returns existing ID if label already exists, otherwise creates new ID.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nexus_core::catalog::Catalog;
    /// # let mut catalog = Catalog::new("./data/catalog").unwrap();
    /// let person_id = catalog.get_or_create_label("Person").unwrap();
    /// let same_id = catalog.get_or_create_label("Person").unwrap();
    /// assert_eq!(person_id, same_id);
    /// ```
    pub fn get_or_create_label(&self, label: &str) -> Result<LabelId> {
        // Try cache first (lock-free).
        if let Some(id) = self.label_name_cache.get(label) {
            return Ok(*id);
        }

        // Need to create new ID — acquire write lock.
        let mut wtxn = self.env.write_txn()?;

        // Double-check in case another thread created it.
        if let Some(id) = self.label_name_to_id.get(&wtxn, label)? {
            // Update cache.
            self.label_name_cache.insert(label.to_string(), id);
            self.label_id_cache.insert(id, label.to_string());
            return Ok(id);
        }

        // Allocate new ID atomically from committed LMDB state within this
        // write txn. Multiple `Catalog` instances can share ONE LMDB env (the
        // shared test catalog, and any future concurrent use); a per-instance
        // in-memory counter hands out the SAME id from two instances, so two
        // distinct labels collide on one id and `get_nodes(id)` returns nodes
        // of both labels. Deriving the id from the LMDB max inside the write
        // txn (LMDB serialises writers across instances/processes) guarantees
        // uniqueness. The in-memory counter is kept monotonic for other readers.
        let id = self.alloc_label_id(&wtxn)?;

        // Insert bidirectional mappings.
        self.label_name_to_id.put(&mut wtxn, label, &id)?;
        self.label_id_to_name.put(&mut wtxn, &id, label)?;

        wtxn.commit()?;

        // Update cache.
        self.label_name_cache.insert(label.to_string(), id);
        self.label_id_cache.insert(id, label.to_string());

        Ok(id)
    }

    /// Phase 1.5.2: Batch get or create multiple labels in a single
    /// transaction.  This reduces I/O overhead when creating multiple labels
    /// at once.
    pub fn batch_get_or_create_labels(
        &self,
        labels: &[&str],
    ) -> Result<std::collections::HashMap<String, LabelId>> {
        let mut result = std::collections::HashMap::new();

        if labels.is_empty() {
            return Ok(result);
        }

        // First pass: check cache for existing labels.
        let mut labels_to_create = Vec::new();
        for label in labels {
            if let Some(id) = self.label_name_cache.get(*label) {
                result.insert(label.to_string(), *id);
            } else {
                labels_to_create.push(*label);
            }
        }

        if labels_to_create.is_empty() {
            return Ok(result);
        }

        // Second pass: create missing labels in a single transaction.
        let mut wtxn = self.env.write_txn()?;

        for label in &labels_to_create {
            // Double-check in case another thread created it.
            if let Some(id) = self.label_name_to_id.get(&wtxn, *label)? {
                // Update cache.
                self.label_name_cache.insert(label.to_string(), id);
                self.label_id_cache.insert(id, label.to_string());
                result.insert(label.to_string(), id);
            } else {
                // Allocate new ID atomically from LMDB state within the txn.
                // Reads in this write txn see prior puts in the same loop, so
                // successive allocations get distinct ids.
                let id = self.alloc_label_id(&wtxn)?;

                // Insert bidirectional mappings.
                self.label_name_to_id.put(&mut wtxn, *label, &id)?;
                self.label_id_to_name.put(&mut wtxn, &id, *label)?;

                // Update cache.
                self.label_name_cache.insert(label.to_string(), id);
                self.label_id_cache.insert(id, label.to_string());
                result.insert(label.to_string(), id);
            }
        }

        wtxn.commit()?;

        Ok(result)
    }

    /// List all `(label_id, label_name)` pairs known to the catalog.
    ///
    /// Mirrors [`list_all_keys`] — reads from LMDB, skips rows that fail to
    /// decode rather than propagating errors so the caller gets the best-
    /// effort snapshot even if a single row is corrupted.
    pub fn list_all_labels(&self) -> Vec<(LabelId, String)> {
        let Ok(rtxn) = self.env.read_txn() else {
            return Vec::new();
        };
        let Ok(iter) = self.label_id_to_name.iter(&rtxn) else {
            return Vec::new();
        };
        iter.filter_map(|r| r.ok())
            .map(|(id, name)| (id, name.to_string()))
            .collect()
    }

    /// Get label name by ID.
    pub fn get_label_name(&self, id: LabelId) -> Result<Option<String>> {
        // Try cache first (lock-free).
        if let Some(name) = self.label_id_cache.get(&id) {
            return Ok(Some(name.clone()));
        }

        let rtxn = self.env.read_txn()?;
        if let Some(name) = self.label_id_to_name.get(&rtxn, &id)? {
            let name_str = name.to_string();
            // Update cache.
            self.label_id_cache.insert(id, name_str.clone());
            return Ok(Some(name_str));
        }
        Ok(None)
    }

    /// Get label ID by name.
    pub fn get_label_id(&self, label: &str) -> Result<LabelId> {
        // Try cache first (lock-free).
        if let Some(id) = self.label_name_cache.get(label) {
            return Ok(*id);
        }

        let rtxn = self.env.read_txn()?;
        match self.label_name_to_id.get(&rtxn, label)? {
            Some(id) => {
                // Update cache.
                self.label_name_cache.insert(label.to_string(), id);
                self.label_id_cache.insert(id, label.to_string());
                Ok(id)
            }
            None => Err(Error::NotFound(format!("Label '{}' not found", label))),
        }
    }

    /// Get label ID by ID (for internal use).
    pub fn get_label_id_by_id(&self, id: LabelId) -> Result<LabelId> {
        // This is a simple identity function for now.
        // In a full implementation, this might do validation.
        Ok(id)
    }

    /// Convert a label bitmap to a vector of label names.
    pub fn get_labels_from_bitmap(&self, bitmap: u64) -> Result<Vec<String>> {
        let mut labels = Vec::new();

        // Check each bit in the bitmap (up to 64 labels).
        for bit in 0..64 {
            if (bitmap & (1u64 << bit)) != 0 {
                let label_id = bit as LabelId;
                if let Some(label_name) = self.get_label_name(label_id)? {
                    labels.push(label_name);
                }
            }
        }

        Ok(labels)
    }

    // ── Type methods ────────────────────────────────────────────────────────

    /// Get or create a type ID.
    ///
    /// Returns existing ID if type already exists, otherwise creates new ID.
    pub fn get_or_create_type(&self, type_name: &str) -> Result<TypeId> {
        // Try cache first (lock-free).
        if let Some(id) = self.type_name_cache.get(type_name) {
            return Ok(*id);
        }

        // Need to create new ID — acquire write lock.
        let mut wtxn = self.env.write_txn()?;

        // Double-check in case another thread created it.
        if let Some(id) = self.type_name_to_id.get(&wtxn, type_name)? {
            // Update cache.
            self.type_name_cache.insert(type_name.to_string(), id);
            self.type_id_cache.insert(id, type_name.to_string());
            return Ok(id);
        }

        // Allocate new ID atomically from LMDB state within this write txn
        // (see `alloc_label_id` — prevents duplicate ids across Catalog
        // instances sharing one env).
        let id = self.alloc_type_id(&wtxn)?;

        // Insert bidirectional mappings.
        self.type_name_to_id.put(&mut wtxn, type_name, &id)?;
        self.type_id_to_name.put(&mut wtxn, &id, type_name)?;

        wtxn.commit()?;

        // Update cache.
        self.type_name_cache.insert(type_name.to_string(), id);
        self.type_id_cache.insert(id, type_name.to_string());

        Ok(id)
    }

    /// Phase 1.5.2: Batch get or create multiple types in a single
    /// transaction.  This reduces I/O overhead when creating multiple types
    /// at once.
    pub fn batch_get_or_create_types(
        &self,
        types: &[&str],
    ) -> Result<std::collections::HashMap<String, TypeId>> {
        let mut result = std::collections::HashMap::new();

        if types.is_empty() {
            return Ok(result);
        }

        // First pass: check cache for existing types.
        let mut types_to_create = Vec::new();
        for type_name in types {
            if let Some(id) = self.type_name_cache.get(*type_name) {
                result.insert(type_name.to_string(), *id);
            } else {
                types_to_create.push(*type_name);
            }
        }

        if types_to_create.is_empty() {
            return Ok(result);
        }

        // Second pass: create missing types in a single transaction.
        let mut wtxn = self.env.write_txn()?;

        for type_name in &types_to_create {
            // Double-check in case another thread created it.
            if let Some(id) = self.type_name_to_id.get(&wtxn, *type_name)? {
                // Update cache.
                self.type_name_cache.insert(type_name.to_string(), id);
                self.type_id_cache.insert(id, type_name.to_string());
                result.insert(type_name.to_string(), id);
            } else {
                // Allocate new ID atomically from LMDB state within the txn.
                let id = self.alloc_type_id(&wtxn)?;

                // Insert bidirectional mappings.
                self.type_name_to_id.put(&mut wtxn, *type_name, &id)?;
                self.type_id_to_name.put(&mut wtxn, &id, *type_name)?;

                // Update cache.
                self.type_name_cache.insert(type_name.to_string(), id);
                self.type_id_cache.insert(id, type_name.to_string());
                result.insert(type_name.to_string(), id);
            }
        }

        wtxn.commit()?;

        Ok(result)
    }

    /// List all `(type_id, type_name)` pairs known to the catalog.
    ///
    /// Mirrors [`list_all_keys`] / [`list_all_labels`] — LMDB iteration with
    /// per-row error tolerance.
    pub fn list_all_types(&self) -> Vec<(TypeId, String)> {
        let Ok(rtxn) = self.env.read_txn() else {
            return Vec::new();
        };
        let Ok(iter) = self.type_id_to_name.iter(&rtxn) else {
            return Vec::new();
        };
        iter.filter_map(|r| r.ok())
            .map(|(id, name)| (id, name.to_string()))
            .collect()
    }

    /// Get type name by ID.
    pub fn get_type_name(&self, id: TypeId) -> Result<Option<String>> {
        // Try cache first (lock-free).
        if let Some(name) = self.type_id_cache.get(&id) {
            return Ok(Some(name.clone()));
        }

        let rtxn = self.env.read_txn()?;
        if let Some(name) = self.type_id_to_name.get(&rtxn, &id)? {
            let name_str = name.to_string();
            // Update cache.
            self.type_id_cache.insert(id, name_str.clone());
            return Ok(Some(name_str));
        }
        Ok(None)
    }

    /// Get type ID by name (returns `None` if type doesn't exist).
    pub fn get_type_id(&self, type_name: &str) -> Result<Option<TypeId>> {
        // Try cache first (lock-free).
        if let Some(id) = self.type_name_cache.get(type_name) {
            return Ok(Some(*id));
        }

        let rtxn = self.env.read_txn()?;
        if let Some(id) = self.type_name_to_id.get(&rtxn, type_name)? {
            // Update cache.
            self.type_name_cache.insert(type_name.to_string(), id);
            self.type_id_cache.insert(id, type_name.to_string());
            return Ok(Some(id));
        }
        Ok(None)
    }

    // ── Key methods ─────────────────────────────────────────────────────────

    /// Get or create a key ID.
    ///
    /// Returns existing ID if key already exists, otherwise creates new ID.
    pub fn get_or_create_key(&self, key: &str) -> Result<KeyId> {
        // Try cache first (lock-free).
        if let Some(id) = self.key_name_cache.get(key) {
            return Ok(*id);
        }

        // Need to create new ID — acquire write lock.
        let mut wtxn = self.env.write_txn()?;

        // Double-check in case another thread created it.
        if let Some(id) = self.key_name_to_id.get(&wtxn, key)? {
            // Update cache.
            self.key_name_cache.insert(key.to_string(), id);
            self.key_id_cache.insert(id, key.to_string());
            return Ok(id);
        }

        // Allocate new ID atomically from LMDB state within this write txn
        // (see `alloc_label_id`).
        let id = self.alloc_key_id(&wtxn)?;

        // Insert bidirectional mappings.
        self.key_name_to_id.put(&mut wtxn, key, &id)?;
        self.key_id_to_name.put(&mut wtxn, &id, key)?;

        wtxn.commit()?;

        // Update cache.
        self.key_name_cache.insert(key.to_string(), id);
        self.key_id_cache.insert(id, key.to_string());

        Ok(id)
    }

    /// Best-effort registration of every top-level key of a property
    /// object with the key name↔id mapping ([`Self::get_or_create_key`])
    /// so `db.propertyKeys()` (and any other catalog-driven key
    /// introspection) can see it. `properties` is typically a node's or
    /// relationship's freshly-written property map; non-`Object` values
    /// (`Null`, an empty map, ...) are a silent no-op.
    ///
    /// Call this at every place a node/relationship property map is
    /// persisted — mirroring the existing per-write
    /// [`Self::get_or_create_label`] / [`Self::get_or_create_type`] calls
    /// already made at those same call sites. Before this method
    /// existed, `get_or_create_key` was reachable only from DDL
    /// (`CREATE INDEX` / `CREATE CONSTRAINT`), so a database with no
    /// index/constraint ever created had an empty key mapping even
    /// though every node/relationship carried named properties —
    /// `db.propertyKeys()` returned nothing.
    ///
    /// A single failed key registration is logged and skipped rather
    /// than propagated: this is bookkeeping for introspection, not
    /// something that should ever abort a write. Each call is a
    /// lock-free [`DashMap`](dashmap::DashMap) cache hit for every key
    /// name already seen at least once anywhere in the database — the
    /// LMDB write-txn path in [`Self::get_or_create_key`] only runs the
    /// first time a given name is observed.
    pub fn register_property_keys(&self, properties: &serde_json::Value) {
        let Some(map) = properties.as_object() else {
            return;
        };
        for key in map.keys() {
            if let Err(e) = self.get_or_create_key(key) {
                tracing::warn!("failed to register property key '{key}' in catalog: {e}");
            }
        }
    }

    /// Get key ID by name.
    pub fn get_key_id(&self, key: &str) -> Result<KeyId> {
        // Try cache first (lock-free).
        if let Some(id) = self.key_name_cache.get(key) {
            return Ok(*id);
        }

        let rtxn = self.env.read_txn()?;
        match self.key_name_to_id.get(&rtxn, key)? {
            Some(id) => {
                // Update cache.
                self.key_name_cache.insert(key.to_string(), id);
                self.key_id_cache.insert(id, key.to_string());
                Ok(id)
            }
            None => Err(Error::NotFound(format!("Key '{}' not found", key))),
        }
    }

    /// Get key name by ID.
    pub fn get_key_name(&self, id: KeyId) -> Result<Option<String>> {
        // Try cache first (lock-free).
        if let Some(name) = self.key_id_cache.get(&id) {
            return Ok(Some(name.clone()));
        }

        let rtxn = self.env.read_txn()?;
        if let Some(name) = self.key_id_to_name.get(&rtxn, &id)? {
            let name_str = name.to_string();
            // Update cache.
            self.key_id_cache.insert(id, name_str.clone());
            return Ok(Some(name_str));
        }
        Ok(None)
    }

    /// List all property keys.
    pub fn list_all_keys(&self) -> Vec<(KeyId, String)> {
        let Ok(rtxn) = self.env.read_txn() else {
            return Vec::new();
        };

        let Ok(iter) = self.key_id_to_name.iter(&rtxn) else {
            return Vec::new();
        };

        iter.filter_map(|r| r.ok())
            .map(|(id, name)| (id, name.to_string()))
            .collect()
    }

    // ── Constraint manager ──────────────────────────────────────────────────

    /// Get constraint manager.
    pub fn constraint_manager(
        &self,
    ) -> &std::sync::Arc<parking_lot::RwLock<crate::catalog::constraints::ConstraintManager>> {
        &self.constraint_manager
    }
}
