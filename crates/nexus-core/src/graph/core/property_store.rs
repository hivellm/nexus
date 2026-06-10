//! Property storage for managing property chains

use crate::graph::simple::PropertyValue;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Property record for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PropertyRecord {
    /// Key of the property
    pub(super) key: String,
    /// Value of the property
    pub(super) value: PropertyValue,
    /// Pointer to next property in chain (u64::MAX if last)
    pub(super) next_ptr: u64,
}

/// Property storage for managing property chains
#[derive(Debug)]
pub(super) struct PropertyStore {
    /// In-memory property storage (in real implementation, this would be persistent)
    pub(super) properties: HashMap<u64, PropertyRecord>,
    /// Next available property pointer
    pub(super) next_ptr: u64,
}

impl PropertyStore {
    pub(super) fn new() -> Self {
        Self {
            properties: HashMap::new(),
            next_ptr: 1,
        }
    }

    /// Store a property chain and return the head pointer
    pub(super) fn store_properties(&mut self, properties: HashMap<String, PropertyValue>) -> u64 {
        if properties.is_empty() {
            return u64::MAX;
        }

        let mut current_ptr = u64::MAX;

        // Store properties in reverse order to maintain chain structure
        let mut prop_vec: Vec<_> = properties.into_iter().collect();
        prop_vec.reverse();

        for (key, value) in prop_vec {
            let ptr = self.next_ptr;
            self.next_ptr += 1;

            let record = PropertyRecord {
                key,
                value,
                next_ptr: current_ptr,
            };

            self.properties.insert(ptr, record);
            current_ptr = ptr;
        }

        current_ptr
    }

    /// Load properties from a property chain
    pub(super) fn load_properties(&self, head_ptr: u64) -> Result<HashMap<String, PropertyValue>> {
        let mut properties = HashMap::new();
        let mut current_ptr = head_ptr;

        while current_ptr != u64::MAX {
            if let Some(record) = self.properties.get(&current_ptr) {
                properties.insert(record.key.clone(), record.value.clone());
                current_ptr = record.next_ptr;
            } else {
                return Err(Error::Storage(format!(
                    "Property record not found at pointer {}",
                    current_ptr
                )));
            }
        }

        Ok(properties)
    }
}
