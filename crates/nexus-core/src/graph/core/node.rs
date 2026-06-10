//! Node data structure

use crate::graph::simple::PropertyValue;
use std::collections::HashMap;

/// A node in the graph with labels and properties
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Node {
    /// Unique identifier for this node
    pub id: super::ids::NodeId,
    /// Labels associated with this node
    pub labels: Vec<String>,
    /// Properties of this node
    pub properties: HashMap<String, PropertyValue>,
}

impl Node {
    /// Create a new node with the given ID and labels
    pub fn new(id: super::ids::NodeId, labels: Vec<String>) -> Self {
        Self {
            id,
            labels,
            properties: HashMap::new(),
        }
    }

    /// Create a new node with ID, labels, and properties
    pub fn with_properties(
        id: super::ids::NodeId,
        labels: Vec<String>,
        properties: HashMap<String, PropertyValue>,
    ) -> Self {
        Self {
            id,
            labels,
            properties,
        }
    }

    /// Add a label to this node
    pub fn add_label(&mut self, label: String) {
        if !self.labels.contains(&label) {
            self.labels.push(label);
        }
    }

    /// Remove a label from this node
    pub fn remove_label(&mut self, label: &str) -> bool {
        if let Some(pos) = self.labels.iter().position(|l| l == label) {
            self.labels.remove(pos);
            true
        } else {
            false
        }
    }

    /// Check if this node has a specific label
    pub fn has_label(&self, label: &str) -> bool {
        self.labels.contains(&label.to_string())
    }

    /// Set a property on this node
    pub fn set_property(&mut self, key: String, value: PropertyValue) {
        self.properties.insert(key, value);
    }

    /// Get a property from this node
    pub fn get_property(&self, key: &str) -> Option<&PropertyValue> {
        self.properties.get(key)
    }

    /// Remove a property from this node
    pub fn remove_property(&mut self, key: &str) -> Option<PropertyValue> {
        self.properties.remove(key)
    }

    /// Check if this node has a specific property
    pub fn has_property(&self, key: &str) -> bool {
        self.properties.contains_key(key)
    }

    /// Get all property keys
    pub fn property_keys(&self) -> Vec<&String> {
        self.properties.keys().collect()
    }

    /// Check if this node is empty (no labels and no properties)
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty() && self.properties.is_empty()
    }
}
