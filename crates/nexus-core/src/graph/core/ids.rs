//! Node and edge identifier types

/// A unique identifier for nodes in the graph
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct NodeId(pub u64);

impl NodeId {
    /// Create a new node ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for NodeId {
    fn from(id: u64) -> Self {
        Self::new(id)
    }
}

impl From<NodeId> for u64 {
    fn from(node_id: NodeId) -> Self {
        node_id.0
    }
}

/// A unique identifier for relationships in the graph
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct EdgeId(pub u64);

impl EdgeId {
    /// Create a new edge ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for EdgeId {
    fn from(id: u64) -> Self {
        Self::new(id)
    }
}

impl From<EdgeId> for u64 {
    fn from(edge_id: EdgeId) -> Self {
        edge_id.0
    }
}
