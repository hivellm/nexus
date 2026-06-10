//! Edge data structure

use crate::graph::simple::PropertyValue;
use std::collections::HashMap;

/// An edge (relationship) in the graph connecting two nodes
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    /// Unique identifier for this edge
    pub id: super::ids::EdgeId,
    /// Source node ID
    pub source: super::ids::NodeId,
    /// Target node ID
    pub target: super::ids::NodeId,
    /// Type of this relationship
    pub relationship_type: String,
    /// Properties of this edge
    pub properties: HashMap<String, PropertyValue>,
}

impl Edge {
    /// Create a new edge with the given ID, source, target, and type
    pub fn new(
        id: super::ids::EdgeId,
        source: super::ids::NodeId,
        target: super::ids::NodeId,
        relationship_type: String,
    ) -> Self {
        Self {
            id,
            source,
            target,
            relationship_type,
            properties: HashMap::new(),
        }
    }

    /// Create a new edge with ID, source, target, type, and properties
    pub fn with_properties(
        id: super::ids::EdgeId,
        source: super::ids::NodeId,
        target: super::ids::NodeId,
        relationship_type: String,
        properties: HashMap<String, PropertyValue>,
    ) -> Self {
        Self {
            id,
            source,
            target,
            relationship_type,
            properties,
        }
    }

    /// Set a property on this edge
    pub fn set_property(&mut self, key: String, value: PropertyValue) {
        self.properties.insert(key, value);
    }

    /// Get a property from this edge
    pub fn get_property(&self, key: &str) -> Option<&PropertyValue> {
        self.properties.get(key)
    }

    /// Remove a property from this edge
    pub fn remove_property(&mut self, key: &str) -> Option<PropertyValue> {
        self.properties.remove(key)
    }

    /// Check if this edge has a specific property
    pub fn has_property(&self, key: &str) -> bool {
        self.properties.contains_key(key)
    }

    /// Get all property keys
    pub fn property_keys(&self) -> Vec<&String> {
        self.properties.keys().collect()
    }

    /// Check if this edge is empty (no properties)
    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }

    /// Get the other end of this edge given one node
    pub fn other_end(&self, node_id: super::ids::NodeId) -> Option<super::ids::NodeId> {
        if self.source == node_id {
            Some(self.target)
        } else if self.target == node_id {
            Some(self.source)
        } else {
            None
        }
    }
}
