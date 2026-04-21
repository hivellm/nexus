//! Composite B-tree index (phase6_opencypher-advanced-types §3).
//!
//! Keys on a tuple of property values under a single label. A user
//! writes
//!
//! ```cypher
//! CREATE INDEX person_tenant_id FOR (p:Person) ON (p.tenantId, p.id)
//! ```
//!
//! and the planner seeks through this index whenever a query predicates
//! the index columns — either a full prefix (equality on every column),
//! a strict prefix (equality on leading columns), or a strict prefix
//! plus a range on the first unbound column. Uniqueness is an optional
//! flag, checked on write with `ERR_CONSTRAINT_VIOLATED`.
//!
//! Representation-wise, the index is a `BTreeMap` keyed by the ordered
//! list of [`super::PropertyValue`]s. The ordering is lexicographic in
//! tuple order, which falls out for free from the `Ord` derive on
//! `Vec<PropertyValue>` because `PropertyValue` already implements
//! `Ord` (handling cross-type ordering the same way the single-column
//! B-tree does).
//!
//! The index is deliberately in-memory — it is rebuilt at startup from
//! the record store (same as [`super::PropertyIndex`]) and persists no
//! on-disk artefact of its own. Durability is owed to the WAL, not to
//! the index.

use super::PropertyValue;
use crate::{Error, Result};
use parking_lot::RwLock;
use roaring::RoaringBitmap;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// A single composite index instance. One per
/// `(label, property_list)` pair; multiple composite indexes per
/// label are allowed (different property lists or different unique
/// flags).
pub struct CompositeBtreeIndex {
    /// Optional user-supplied name (`CREATE INDEX <name> FOR ...`).
    pub name: Option<String>,
    /// The label every indexed node carries. Stored by catalog ID so
    /// we don't pay a string lookup on every seek.
    pub label_id: u32,
    /// Ordered property keys the index is defined over. Length ≥ 1.
    pub property_keys: Vec<String>,
    /// UNIQUE flag — if set, a write that would produce a duplicate
    /// key is rejected.
    pub unique: bool,
    /// The actual B-tree. Key = ordered tuple of values, value = set
    /// of matching node IDs.
    tree: BTreeMap<Vec<PropertyValue>, RoaringBitmap>,
}

impl CompositeBtreeIndex {
    /// Build a fresh, empty composite index.
    pub fn new(
        name: Option<String>,
        label_id: u32,
        property_keys: Vec<String>,
        unique: bool,
    ) -> Result<Self> {
        if property_keys.is_empty() {
            return Err(Error::storage(
                "composite index requires at least one property key".to_string(),
            ));
        }
        Ok(Self {
            name,
            label_id,
            property_keys,
            unique,
            tree: BTreeMap::new(),
        })
    }

    /// Insert a node-id under the supplied tuple of property values.
    /// Caller is responsible for handing in exactly
    /// `self.property_keys.len()` values, in the same order.
    pub fn insert(&mut self, node_id: u64, values: Vec<PropertyValue>) -> Result<()> {
        if values.len() != self.property_keys.len() {
            return Err(Error::storage(format!(
                "composite index arity mismatch: expected {}, got {}",
                self.property_keys.len(),
                values.len()
            )));
        }
        if self.unique {
            if let Some(existing) = self.tree.get(&values) {
                if !existing.is_empty() && !existing.contains(node_id as u32) {
                    return Err(Error::CypherExecution(
                        "ERR_CONSTRAINT_VIOLATED: composite UNIQUE index violated".to_string(),
                    ));
                }
            }
        }
        self.tree.entry(values).or_default().insert(node_id as u32);
        Ok(())
    }

    /// Remove a node-id from its tuple slot. No-op if the tuple is
    /// not present.
    pub fn remove(&mut self, node_id: u64, values: &[PropertyValue]) {
        if let Some(bitmap) = self.tree.get_mut(values) {
            bitmap.remove(node_id as u32);
            if bitmap.is_empty() {
                self.tree.remove(values);
            }
        }
    }

    /// Point seek: every column equality-bound. Returns the node-ids
    /// sharing the exact tuple, empty if the tuple is not indexed.
    pub fn seek_exact(&self, values: &[PropertyValue]) -> Vec<u64> {
        self.tree
            .get(values)
            .map(|bm| bm.iter().map(|n| n as u64).collect())
            .unwrap_or_default()
    }

    /// Prefix seek: leading `prefix.len()` columns equality-bound, the
    /// rest open. Returns the union of all node-ids whose key starts
    /// with the supplied prefix.
    pub fn seek_prefix(&self, prefix: &[PropertyValue]) -> Vec<u64> {
        if prefix.len() > self.property_keys.len() {
            return Vec::new();
        }
        let mut out = RoaringBitmap::new();
        for (k, v) in self.tree.iter() {
            if k.len() < prefix.len() {
                continue;
            }
            if k[..prefix.len()] == prefix[..] {
                out |= v;
            }
        }
        out.iter().map(|n| n as u64).collect()
    }

    /// Range seek: first `prefix.len()` columns equality-bound, the
    /// column at index `prefix.len()` constrained to `[lo, hi]` with
    /// the inclusivity flags.
    ///
    /// Kept simple (linear filter rather than a sub-tree split) — the
    /// tree is small enough in practice that a tighter bound pays for
    /// itself only in multi-million-key scenarios, which the heuristic
    /// planner does not yet produce for composite indexes.
    pub fn seek_range(
        &self,
        prefix: &[PropertyValue],
        lo: Option<(PropertyValue, bool)>,
        hi: Option<(PropertyValue, bool)>,
    ) -> Vec<u64> {
        if prefix.len() >= self.property_keys.len() {
            return self.seek_exact(prefix);
        }
        let mut out = RoaringBitmap::new();
        for (k, v) in self.tree.iter() {
            if k.len() <= prefix.len() {
                continue;
            }
            if k[..prefix.len()] != prefix[..] {
                continue;
            }
            let col = &k[prefix.len()];
            if let Some((lo_v, lo_inc)) = &lo {
                match col.cmp(lo_v) {
                    std::cmp::Ordering::Less => continue,
                    std::cmp::Ordering::Equal if !lo_inc => continue,
                    _ => {}
                }
            }
            if let Some((hi_v, hi_inc)) = &hi {
                match col.cmp(hi_v) {
                    std::cmp::Ordering::Greater => continue,
                    std::cmp::Ordering::Equal if !hi_inc => continue,
                    _ => {}
                }
            }
            out |= v;
        }
        out.iter().map(|n| n as u64).collect()
    }

    /// Total number of tuples in the index (distinct composite keys).
    pub fn entry_count(&self) -> usize {
        self.tree.len()
    }

    /// Total number of node-ids across all tuples.
    pub fn node_count(&self) -> usize {
        self.tree.values().map(|v| v.len() as usize).sum()
    }
}

/// Registry for every composite index on the engine. Keyed by label-id
/// then property-list so lookups during seek are O(log L) where L is
/// the number of composite indexes on the label.
#[derive(Clone, Default)]
pub struct CompositeBtreeRegistry {
    indexes: Arc<RwLock<HashMap<u32, Vec<Arc<RwLock<CompositeBtreeIndex>>>>>>,
}

impl CompositeBtreeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new composite index. Returns an `Err` if an index
    /// with the same label and property list already exists (unless
    /// the caller passed `if_not_exists`, in which case the call is a
    /// no-op for the conflict).
    pub fn register(
        &self,
        label_id: u32,
        property_keys: Vec<String>,
        unique: bool,
        name: Option<String>,
        if_not_exists: bool,
    ) -> Result<()> {
        let mut map = self.indexes.write();
        let entry = map.entry(label_id).or_default();
        if entry.iter().any(|idx| {
            let g = idx.read();
            g.property_keys == property_keys && g.unique == unique
        }) {
            if if_not_exists {
                return Ok(());
            }
            return Err(Error::storage(format!(
                "composite index on label {label_id:?} with keys {property_keys:?} already exists"
            )));
        }
        let idx = CompositeBtreeIndex::new(name, label_id, property_keys, unique)?;
        entry.push(Arc::new(RwLock::new(idx)));
        Ok(())
    }

    /// Find a composite index by exact (label, property_keys) match.
    pub fn find(
        &self,
        label_id: u32,
        property_keys: &[String],
    ) -> Option<Arc<RwLock<CompositeBtreeIndex>>> {
        self.indexes.read().get(&label_id).and_then(|v| {
            v.iter()
                .find(|i| i.read().property_keys == property_keys)
                .cloned()
        })
    }

    /// Enumerate every composite index for reporting through
    /// `db.indexes()`. Each entry yields
    /// `(label_id, property_keys, unique, name)`.
    pub fn list(&self) -> Vec<(u32, Vec<String>, bool, Option<String>)> {
        self.indexes
            .read()
            .values()
            .flatten()
            .map(|idx| {
                let g = idx.read();
                (
                    g.label_id,
                    g.property_keys.clone(),
                    g.unique,
                    g.name.clone(),
                )
            })
            .collect()
    }

    /// Drop a composite index. Returns `true` if an index was removed,
    /// `false` otherwise.
    pub fn drop_index(&self, label_id: u32, property_keys: &[String]) -> bool {
        let mut map = self.indexes.write();
        if let Some(v) = map.get_mut(&label_id) {
            let before = v.len();
            v.retain(|idx| idx.read().property_keys != property_keys);
            return v.len() != before;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pv_int(x: i64) -> PropertyValue {
        PropertyValue::Integer(x)
    }
    fn pv_str(s: &str) -> PropertyValue {
        PropertyValue::String(s.to_string())
    }

    #[test]
    fn exact_seek_returns_matching_nodes() {
        let mut idx = CompositeBtreeIndex::new(
            None,
            1,
            vec!["tenantId".to_string(), "id".to_string()],
            false,
        )
        .unwrap();
        idx.insert(10, vec![pv_str("a"), pv_int(1)]).unwrap();
        idx.insert(11, vec![pv_str("a"), pv_int(2)]).unwrap();
        idx.insert(12, vec![pv_str("b"), pv_int(1)]).unwrap();

        let mut res = idx.seek_exact(&[pv_str("a"), pv_int(1)]);
        res.sort();
        assert_eq!(res, vec![10]);
    }

    #[test]
    fn prefix_seek_returns_all_matching_prefix() {
        let mut idx = CompositeBtreeIndex::new(
            None,
            1,
            vec!["tenantId".to_string(), "id".to_string()],
            false,
        )
        .unwrap();
        idx.insert(10, vec![pv_str("a"), pv_int(1)]).unwrap();
        idx.insert(11, vec![pv_str("a"), pv_int(2)]).unwrap();
        idx.insert(12, vec![pv_str("b"), pv_int(1)]).unwrap();

        let mut res = idx.seek_prefix(&[pv_str("a")]);
        res.sort();
        assert_eq!(res, vec![10, 11]);
    }

    #[test]
    fn range_seek_with_prefix() {
        let mut idx = CompositeBtreeIndex::new(
            None,
            1,
            vec!["tenantId".to_string(), "id".to_string()],
            false,
        )
        .unwrap();
        for n in 100..110 {
            idx.insert(n as u64, vec![pv_str("a"), pv_int(n)]).unwrap();
        }
        idx.insert(999, vec![pv_str("b"), pv_int(105)]).unwrap();

        let mut res = idx.seek_range(
            &[pv_str("a")],
            Some((pv_int(100), false)),
            Some((pv_int(200), false)),
        );
        res.sort();
        assert_eq!(res, vec![101, 102, 103, 104, 105, 106, 107, 108, 109]);
    }

    #[test]
    fn unique_violation_rejected() {
        let mut idx =
            CompositeBtreeIndex::new(None, 1, vec!["k".to_string(), "v".to_string()], true)
                .unwrap();
        idx.insert(1, vec![pv_str("a"), pv_int(1)]).unwrap();
        let err = idx.insert(2, vec![pv_str("a"), pv_int(1)]).unwrap_err();
        assert!(err.to_string().contains("ERR_CONSTRAINT_VIOLATED"));
    }

    #[test]
    fn reinsert_same_node_is_idempotent_for_unique() {
        let mut idx = CompositeBtreeIndex::new(None, 1, vec!["k".to_string()], true).unwrap();
        idx.insert(1, vec![pv_str("a")]).unwrap();
        idx.insert(1, vec![pv_str("a")]).unwrap();
        assert_eq!(idx.node_count(), 1);
    }

    #[test]
    fn arity_mismatch_rejected() {
        let mut idx =
            CompositeBtreeIndex::new(None, 1, vec!["a".to_string(), "b".to_string()], false)
                .unwrap();
        assert!(idx.insert(1, vec![pv_str("a")]).is_err());
    }

    #[test]
    fn remove_empty_then_seek_empty() {
        let mut idx = CompositeBtreeIndex::new(None, 1, vec!["k".to_string()], false).unwrap();
        idx.insert(1, vec![pv_int(5)]).unwrap();
        idx.remove(1, &[pv_int(5)]);
        assert!(idx.seek_exact(&[pv_int(5)]).is_empty());
    }

    #[test]
    fn registry_dedup_and_if_not_exists() {
        let reg = CompositeBtreeRegistry::new();
        reg.register(
            1,
            vec!["a".to_string(), "b".to_string()],
            false,
            None,
            false,
        )
        .unwrap();
        assert!(
            reg.register(
                1,
                vec!["a".to_string(), "b".to_string()],
                false,
                None,
                false
            )
            .is_err()
        );
        // if_not_exists flag makes the conflict silent
        reg.register(1, vec!["a".to_string(), "b".to_string()], false, None, true)
            .unwrap();
        // Different unique flag = distinct index
        reg.register(1, vec!["a".to_string(), "b".to_string()], true, None, false)
            .unwrap();
        assert_eq!(reg.list().len(), 2);
    }
}
