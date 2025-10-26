//! Graph algorithms module
//!
//! This module provides essential graph algorithms including:
//! - Breadth-First Search (BFS)
//! - Depth-First Search (DFS)
//! - Shortest Path algorithms (Dijkstra, A*)
//! - Connected Components
//! - Topological Sort
//! - Minimum Spanning Tree (MST)

use crate::{Error, Result};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

/// Graph representation for algorithms
#[derive(Debug, Clone)]
pub struct Graph {
    /// Adjacency list representation
    /// Key: node_id, Value: list of (neighbor_id, edge_weight)
    adjacency: HashMap<u64, Vec<(u64, f64)>>,
    /// Node labels
    node_labels: HashMap<u64, Vec<String>>,
    /// Edge types
    edge_types: HashMap<(u64, u64), Vec<String>>,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    /// Create a new empty graph
    pub fn new() -> Self {
        Self {
            adjacency: HashMap::new(),
            node_labels: HashMap::new(),
            edge_types: HashMap::new(),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node_id: u64, labels: Vec<String>) {
        self.adjacency.entry(node_id).or_default();
        self.node_labels.insert(node_id, labels);
    }

    /// Add an edge to the graph
    pub fn add_edge(&mut self, from: u64, to: u64, weight: f64, edge_types: Vec<String>) {
        self.adjacency.entry(from).or_default().push((to, weight));
        self.edge_types.insert((from, to), edge_types);
    }

    /// Get neighbors of a node
    pub fn get_neighbors(&self, node_id: u64) -> &[(u64, f64)] {
        self.adjacency
            .get(&node_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get all nodes in the graph
    pub fn get_nodes(&self) -> Vec<u64> {
        self.adjacency.keys().cloned().collect()
    }

    /// Check if a node exists
    pub fn has_node(&self, node_id: u64) -> bool {
        self.adjacency.contains_key(&node_id)
    }

    /// Get node labels
    pub fn get_node_labels(&self, node_id: u64) -> Option<&Vec<String>> {
        self.node_labels.get(&node_id)
    }

    /// Get edge types
    pub fn get_edge_types(&self, from: u64, to: u64) -> Option<&Vec<String>> {
        self.edge_types.get(&(from, to))
    }
}

/// BFS result containing distances and paths
#[derive(Debug, Clone)]
pub struct BfsResult {
    /// Distance from source to each node
    pub distances: HashMap<u64, usize>,
    /// Parent node for each node in the BFS tree
    pub parents: HashMap<u64, u64>,
    /// Order of node discovery
    pub discovery_order: Vec<u64>,
}

/// DFS result containing discovery and finish times
#[derive(Debug, Clone)]
pub struct DfsResult {
    /// Discovery time for each node
    pub discovery_times: HashMap<u64, usize>,
    /// Finish time for each node
    pub finish_times: HashMap<u64, usize>,
    /// Parent node for each node in the DFS tree
    pub parents: HashMap<u64, u64>,
    /// Order of node discovery
    pub discovery_order: Vec<u64>,
}

/// Shortest path result
#[derive(Debug, Clone)]
pub struct ShortestPathResult {
    /// Distance from source to each node
    pub distances: HashMap<u64, f64>,
    /// Parent node for each node in the shortest path tree
    pub parents: HashMap<u64, u64>,
    /// Path from source to target (if target specified)
    pub path: Option<Vec<u64>>,
}

/// Connected components result
#[derive(Debug, Clone)]
pub struct ConnectedComponentsResult {
    /// Component ID for each node
    pub components: HashMap<u64, usize>,
    /// Number of connected components
    pub component_count: usize,
    /// Nodes in each component
    pub component_nodes: HashMap<usize, Vec<u64>>,
}

/// Topological sort result
#[derive(Debug, Clone)]
pub struct TopologicalSortResult {
    /// Topologically sorted nodes
    pub sorted_nodes: Vec<u64>,
    /// Whether the graph has cycles
    pub has_cycle: bool,
}

/// MST result
#[derive(Debug, Clone)]
pub struct MstResult {
    /// Edges in the MST
    pub edges: Vec<(u64, u64, f64)>,
    /// Total weight of the MST
    pub total_weight: f64,
}

/// Graph algorithms implementation
impl Graph {
    /// Perform Breadth-First Search from a source node
    pub fn bfs(&self, source: u64) -> Result<BfsResult> {
        if !self.has_node(source) {
            return Err(Error::InvalidInput("Source node not found".to_string()));
        }

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();
        let mut discovery_order = Vec::new();
        let mut queue = VecDeque::new();

        distances.insert(source, 0);
        queue.push_back(source);
        discovery_order.push(source);

        while let Some(current) = queue.pop_front() {
            let current_distance = distances[&current];

            for &(neighbor, _) in self.get_neighbors(current) {
                if let std::collections::hash_map::Entry::Vacant(e) = distances.entry(neighbor) {
                    e.insert(current_distance + 1);
                    parents.insert(neighbor, current);
                    queue.push_back(neighbor);
                    discovery_order.push(neighbor);
                }
            }
        }

        Ok(BfsResult {
            distances,
            parents,
            discovery_order,
        })
    }

    /// Perform Depth-First Search from a source node
    pub fn dfs(&self, source: u64) -> Result<DfsResult> {
        if !self.has_node(source) {
            return Err(Error::InvalidInput("Source node not found".to_string()));
        }

        let mut discovery_times = HashMap::new();
        let mut finish_times = HashMap::new();
        let mut parents = HashMap::new();
        let mut discovery_order = Vec::new();
        let mut time = 0;

        self.dfs_visit(
            source,
            &mut discovery_times,
            &mut finish_times,
            &mut parents,
            &mut discovery_order,
            &mut time,
        );

        Ok(DfsResult {
            discovery_times,
            finish_times,
            parents,
            discovery_order,
        })
    }

    /// DFS visit helper function
    fn dfs_visit(
        &self,
        node: u64,
        discovery_times: &mut HashMap<u64, usize>,
        finish_times: &mut HashMap<u64, usize>,
        parents: &mut HashMap<u64, u64>,
        discovery_order: &mut Vec<u64>,
        time: &mut usize,
    ) {
        *time += 1;
        discovery_times.insert(node, *time);
        discovery_order.push(node);

        for &(neighbor, _) in self.get_neighbors(node) {
            if !discovery_times.contains_key(&neighbor) {
                parents.insert(neighbor, node);
                self.dfs_visit(
                    neighbor,
                    discovery_times,
                    finish_times,
                    parents,
                    discovery_order,
                    time,
                );
            }
        }

        *time += 1;
        finish_times.insert(node, *time);
    }

    /// Find shortest path using Dijkstra's algorithm
    pub fn dijkstra(&self, source: u64, target: Option<u64>) -> Result<ShortestPathResult> {
        if !self.has_node(source) {
            return Err(Error::InvalidInput("Source node not found".to_string()));
        }

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();
        let mut heap = BinaryHeap::new();

        // Initialize distances
        for &node in self.adjacency.keys() {
            distances.insert(node, f64::INFINITY);
        }
        distances.insert(source, 0.0);

        // Priority queue item (negative distance for max-heap behavior)
        #[derive(PartialEq)]
        struct QueueItem(f64, u64);

        impl Eq for QueueItem {}

        impl PartialOrd for QueueItem {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for QueueItem {
            fn cmp(&self, other: &Self) -> Ordering {
                // Reverse the comparison for min-heap behavior
                other.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
            }
        }

        heap.push(QueueItem(0.0, source));

        while let Some(QueueItem(dist, current)) = heap.pop() {
            if dist > distances[&current] {
                continue;
            }

            for &(neighbor, weight) in self.get_neighbors(current) {
                let new_dist = dist + weight;
                if new_dist < distances[&neighbor] {
                    distances.insert(neighbor, new_dist);
                    parents.insert(neighbor, current);
                    heap.push(QueueItem(new_dist, neighbor));
                }
            }
        }

        // Reconstruct path if target is specified
        let path = if let Some(target) = target {
            self.reconstruct_path(&parents, source, target)
        } else {
            None
        };

        Ok(ShortestPathResult {
            distances,
            parents,
            path,
        })
    }

    /// Find shortest path using A* algorithm
    pub fn astar(
        &self,
        source: u64,
        target: u64,
        heuristic: impl Fn(u64, u64) -> f64,
    ) -> Result<ShortestPathResult> {
        if !self.has_node(source) || !self.has_node(target) {
            return Err(Error::InvalidInput(
                "Source or target node not found".to_string(),
            ));
        }

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();
        let mut heap = BinaryHeap::new();

        // Initialize distances
        for &node in self.adjacency.keys() {
            distances.insert(node, f64::INFINITY);
        }
        distances.insert(source, 0.0);

        // Priority queue item (f_score, g_score, node_id)
        #[derive(PartialEq)]
        struct QueueItem(f64, f64, u64);

        impl Eq for QueueItem {}

        impl PartialOrd for QueueItem {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for QueueItem {
            fn cmp(&self, other: &Self) -> Ordering {
                self.partial_cmp(other).unwrap_or(Ordering::Equal)
            }
        }

        let f_score = 0.0 + heuristic(source, target);
        heap.push(QueueItem(f_score, 0.0, source));

        while let Some(QueueItem(_, g_score, current)) = heap.pop() {
            if current == target {
                break;
            }

            if g_score > distances[&current] {
                continue;
            }

            for &(neighbor, weight) in self.get_neighbors(current) {
                let tentative_g = g_score + weight;
                if tentative_g < distances[&neighbor] {
                    distances.insert(neighbor, tentative_g);
                    parents.insert(neighbor, current);
                    let f_score = tentative_g + heuristic(neighbor, target);
                    heap.push(QueueItem(f_score, tentative_g, neighbor));
                }
            }
        }

        let path = self.reconstruct_path(&parents, source, target);

        Ok(ShortestPathResult {
            distances,
            parents,
            path,
        })
    }

    /// Find connected components using Union-Find
    pub fn connected_components(&self) -> ConnectedComponentsResult {
        let mut components = HashMap::new();
        let mut component_count = 0;
        let mut visited = HashSet::new();

        for &node in self.adjacency.keys() {
            if !visited.contains(&node) {
                let mut component_nodes = Vec::new();
                self.dfs_component(node, &mut visited, &mut component_nodes);

                for &component_node in &component_nodes {
                    components.insert(component_node, component_count);
                }
                component_count += 1;
            }
        }

        // Group nodes by component
        let mut component_nodes = HashMap::new();
        for (node, component_id) in &components {
            component_nodes
                .entry(*component_id)
                .or_insert_with(Vec::new)
                .push(*node);
        }

        ConnectedComponentsResult {
            components,
            component_count,
            component_nodes,
        }
    }

    /// DFS helper for connected components
    fn dfs_component(&self, node: u64, visited: &mut HashSet<u64>, component_nodes: &mut Vec<u64>) {
        visited.insert(node);
        component_nodes.push(node);

        for &(neighbor, _) in self.get_neighbors(node) {
            if !visited.contains(&neighbor) {
                self.dfs_component(neighbor, visited, component_nodes);
            }
        }
    }

    /// Perform topological sort using DFS
    pub fn topological_sort(&self) -> TopologicalSortResult {
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();
        let mut sorted_nodes = Vec::new();
        let mut has_cycle = false;

        for &node in self.adjacency.keys() {
            if !visited.contains(&node)
                && self.dfs_topological(
                    node,
                    &mut visited,
                    &mut temp_visited,
                    &mut sorted_nodes,
                    &mut has_cycle,
                )
            {
                has_cycle = true;
                break;
            }
        }

        sorted_nodes.reverse();

        TopologicalSortResult {
            sorted_nodes,
            has_cycle,
        }
    }

    /// DFS helper for topological sort
    fn dfs_topological(
        &self,
        node: u64,
        visited: &mut HashSet<u64>,
        temp_visited: &mut HashSet<u64>,
        sorted_nodes: &mut Vec<u64>,
        has_cycle: &mut bool,
    ) -> bool {
        if temp_visited.contains(&node) {
            *has_cycle = true;
            return true;
        }

        if visited.contains(&node) {
            return false;
        }

        temp_visited.insert(node);

        for &(neighbor, _) in self.get_neighbors(node) {
            if self.dfs_topological(neighbor, visited, temp_visited, sorted_nodes, has_cycle) {
                return true;
            }
        }

        temp_visited.remove(&node);
        visited.insert(node);
        sorted_nodes.push(node);

        false
    }

    /// Find Minimum Spanning Tree using Kruskal's algorithm
    pub fn minimum_spanning_tree(&self) -> Result<MstResult> {
        let mut edges = Vec::new();

        // Collect all edges
        for (&from, neighbors) in &self.adjacency {
            for &(to, weight) in neighbors {
                if from < to {
                    // Avoid duplicate edges
                    edges.push((from, to, weight));
                }
            }
        }

        // Sort edges by weight
        edges.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));

        // Union-Find data structure
        let mut parent = HashMap::new();
        let mut rank = HashMap::new();

        for &node in self.adjacency.keys() {
            parent.insert(node, node);
            rank.insert(node, 0);
        }

        fn find(parent: &mut HashMap<u64, u64>, x: u64) -> u64 {
            if parent[&x] != x {
                let parent_x = parent[&x];
                let root = find(parent, parent_x);
                parent.insert(x, root);
            }
            parent[&x]
        }

        fn union(
            parent: &mut HashMap<u64, u64>,
            rank: &mut HashMap<u64, u64>,
            x: u64,
            y: u64,
        ) -> bool {
            let px = find(parent, x);
            let py = find(parent, y);

            if px == py {
                return false;
            }

            if rank[&px] < rank[&py] {
                parent.insert(px, py);
            } else if rank[&px] > rank[&py] {
                parent.insert(py, px);
            } else {
                parent.insert(py, px);
                rank.insert(px, rank[&px] + 1);
            }

            true
        }

        let mut mst_edges = Vec::new();
        let mut total_weight = 0.0;

        for (from, to, weight) in edges {
            if union(&mut parent, &mut rank, from, to) {
                mst_edges.push((from, to, weight));
                total_weight += weight;
            }
        }

        Ok(MstResult {
            edges: mst_edges,
            total_weight,
        })
    }

    /// Reconstruct path from parents map
    fn reconstruct_path(
        &self,
        parents: &HashMap<u64, u64>,
        source: u64,
        target: u64,
    ) -> Option<Vec<u64>> {
        if !parents.contains_key(&target) {
            return None;
        }

        let mut path = Vec::new();
        let mut current = target;
        let mut visited = HashSet::new();

        while current != source {
            if visited.contains(&current) {
                // Cycle detected, return None
                return None;
            }
            visited.insert(current);
            path.push(current);
            current = parents[&current];
        }
        path.push(source);
        path.reverse();

        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_creation() {
        let mut graph = Graph::new();
        graph.add_node(1, vec!["Person".to_string()]);
        graph.add_node(2, vec!["Person".to_string()]);
        graph.add_edge(1, 2, 1.0, vec!["KNOWS".to_string()]);

        assert!(graph.has_node(1));
        assert!(graph.has_node(2));
        assert_eq!(graph.get_neighbors(1).len(), 1);
        assert_eq!(graph.get_neighbors(2).len(), 0);
    }

    #[test]
    fn test_bfs() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);

        let result = graph.bfs(1).unwrap();
        assert_eq!(result.distances[&1], 0);
        assert_eq!(result.distances[&2], 1);
        assert_eq!(result.distances[&3], 2);
    }

    #[test]
    fn test_dfs() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);

        let result = graph.dfs(1).unwrap();
        assert!(result.discovery_times.contains_key(&1));
        assert!(result.discovery_times.contains_key(&2));
        assert!(result.discovery_times.contains_key(&3));
    }

    #[test]
    fn test_dijkstra() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 2.0, vec![]);
        graph.add_edge(1, 3, 4.0, vec![]);

        let result = graph.dijkstra(1, Some(3)).unwrap();
        assert_eq!(result.distances[&3], 3.0);
        assert!(result.path.is_some());
    }

    #[test]
    fn test_connected_components() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_node(4, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(3, 4, 1.0, vec![]);

        let result = graph.connected_components();
        assert_eq!(result.component_count, 2);
    }

    #[test]
    fn test_topological_sort() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);

        let result = graph.topological_sort();
        assert!(!result.has_cycle);
        assert_eq!(result.sorted_nodes.len(), 3);
    }

    #[test]
    fn test_minimum_spanning_tree() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 2.0, vec![]);
        graph.add_edge(1, 3, 3.0, vec![]);

        let result = graph.minimum_spanning_tree().unwrap();
        assert_eq!(result.edges.len(), 2);
        assert_eq!(result.total_weight, 3.0);
    }
}
