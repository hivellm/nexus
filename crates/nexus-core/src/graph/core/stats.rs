//! Graph statistics types

/// Helper structures for statistics calculation
#[derive(Debug, Clone)]
pub(super) struct DegreeStats {
    pub(super) avg_degree: f64,
    pub(super) max_degree: usize,
    pub(super) min_degree: usize,
}

#[derive(Debug, Clone)]
pub(super) struct PathStats {
    pub(super) avg_shortest_path_length: f64,
    pub(super) diameter: usize,
}

#[derive(Debug, Clone)]
pub(super) struct NodeTypeStats {
    pub(super) isolated_nodes: usize,
    pub(super) leaf_nodes: usize,
}

#[derive(Debug, Clone)]
pub(super) struct EdgeTypeStats {
    pub(super) self_loops: usize,
    pub(super) bidirectional_edges: usize,
}

/// Statistics about the graph
#[derive(Debug, Clone)]
pub struct GraphStats {
    /// Number of active nodes in the graph
    pub total_nodes: usize,
    /// Number of active edges in the graph
    pub total_edges: usize,
    /// Number of nodes in storage (including deleted)
    pub storage_nodes: usize,
    /// Number of edges in storage (including deleted)
    pub storage_edges: usize,
    /// Number of nodes in cache
    pub cached_nodes: usize,
    /// Number of edges in cache
    pub cached_edges: usize,
    /// Average degree (connections per node)
    pub avg_degree: f64,
    /// Maximum degree of any node
    pub max_degree: usize,
    /// Minimum degree of any node
    pub min_degree: usize,
    /// Graph density (actual edges / possible edges)
    pub graph_density: f64,
    /// Number of connected components
    pub connected_components: usize,
    /// Average clustering coefficient
    pub avg_clustering_coefficient: f64,
    /// Average shortest path length
    pub avg_shortest_path_length: f64,
    /// Graph diameter (longest shortest path)
    pub diameter: usize,
    /// Number of isolated nodes (degree 0)
    pub isolated_nodes: usize,
    /// Number of leaf nodes (degree 1)
    pub leaf_nodes: usize,
    /// Number of self-loops
    pub self_loops: usize,
    /// Number of bidirectional edges
    pub bidirectional_edges: usize,
}
