//! B-tree index implementation for property range queries

use crate::error::{Error, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::RwLock;

/// B-tree index for property range queries
#[derive(Debug)]
pub struct BTreeIndex {
    index: RwLock<BTreeMap<PropertyKey, Vec<u64>>>,
    label_id: u32,
    property_key_id: u32,
    stats: RwLock<IndexStats>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyKey {
    pub value: Value,
    pub node_id: u64,
}

impl PartialOrd for PropertyKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PropertyKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match PropertyKey::compare_values(&self.value, &other.value) {
            x if x < 0 => std::cmp::Ordering::Less,
            x if x > 0 => std::cmp::Ordering::Greater,
            _ => self.node_id.cmp(&other.node_id),
        }
    }
}

impl PropertyKey {
    fn compare_values(a: &Value, b: &Value) -> i32 {
        match (a, b) {
            (Value::Number(a_num), Value::Number(b_num)) => {
                let a_f64 = a_num.as_f64().unwrap_or(0.0);
                let b_f64 = b_num.as_f64().unwrap_or(0.0);
                a_f64
                    .partial_cmp(&b_f64)
                    .unwrap_or(std::cmp::Ordering::Equal) as i32
            }
            (Value::String(a_str), Value::String(b_str)) => a_str.cmp(b_str) as i32,
            (Value::Bool(a_bool), Value::Bool(b_bool)) => (*a_bool as i32) - (*b_bool as i32),
            (Value::Null, Value::Null) => 0,
            (Value::Null, _) => -1,
            (_, Value::Null) => 1,
            _ => format!("{:?}", a).cmp(&format!("{:?}", b)) as i32,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub entry_count: u64,
    pub unique_value_count: u64,
    pub size_bytes: u64,
    pub height: u32,
}

impl BTreeIndex {
    pub fn new(label_id: u32, property_key_id: u32) -> Self {
        Self {
            index: RwLock::new(BTreeMap::new()),
            label_id,
            property_key_id,
            stats: RwLock::new(IndexStats {
                entry_count: 0,
                unique_value_count: 0,
                size_bytes: 0,
                height: 0,
            }),
        }
    }

    pub fn insert(&self, node_id: u64, value: Value) -> Result<()> {
        let key = PropertyKey {
            value: value.clone(),
            node_id,
        };
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut stats = self
            .stats
            .write()
            .map_err(|_| Error::internal("B-tree stats lock poisoned"))?;

        if index.get(&key).is_none() {
            index.insert(key, vec![node_id]);
            stats.entry_count += 1;
            stats.unique_value_count = index.len() as u64;
        }
        Ok(())
    }

    pub fn find_exact(&self, value: &Value) -> Result<Vec<u64>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut results = Vec::new();
        for (key, node_ids) in index.iter() {
            if key.value == *value {
                results.extend(node_ids);
            }
        }
        Ok(results)
    }

    pub fn find_range(&self, min_value: &Value, max_value: &Value) -> Result<Vec<u64>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut results = Vec::new();
        for (key, node_ids) in index.iter() {
            let cmp_min = self.compare_values(&key.value, min_value);
            let cmp_max = self.compare_values(&key.value, max_value);
            if cmp_min >= 0 && cmp_max <= 0 {
                results.extend(node_ids);
            }
        }
        Ok(results)
    }

    fn compare_values(&self, a: &Value, b: &Value) -> i32 {
        PropertyKey::compare_values(a, b)
    }
}

impl Clone for BTreeIndex {
    fn clone(&self) -> Self {
        let index = self.index.read().unwrap();
        let stats = self.stats.read().unwrap();
        Self {
            index: RwLock::new(index.clone()),
            label_id: self.label_id,
            property_key_id: self.property_key_id,
            stats: RwLock::new(stats.clone()),
        }
    }
}
