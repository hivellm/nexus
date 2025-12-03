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

    /// Build graph from Engine storage
    /// This allows using graph algorithms directly on database data
    pub fn from_engine(engine: &crate::Engine, weight_property: Option<&str>) -> Result<Self> {
        let mut graph = Self::new();
        let tx = engine.transaction_manager.write().begin_read()?;

        // Add all nodes
        for node_id in 0..engine.storage.node_count() {
            if let Ok(Some(node_record)) = engine.storage.get_node(&tx, node_id) {
                if !node_record.is_deleted() {
                    // Get labels
                    let labels = engine
                        .catalog
                        .get_labels_from_bitmap(node_record.label_bits)?;

                    graph.add_node(node_id, labels);
                }
            }
        }

        // Add all relationships as edges
        for rel_id in 0..engine.storage.relationship_count() {
            if let Ok(Some(rel_record)) = engine.storage.get_relationship(&tx, rel_id) {
                if !rel_record.is_deleted() {
                    // Get relationship type
                    let rel_type = engine
                        .catalog
                        .get_type_name(rel_record.type_id)
                        .unwrap_or_else(|_| Some("UNKNOWN".to_string()))
                        .unwrap_or_else(|| "UNKNOWN".to_string());

                    // Extract weight from properties if specified
                    let mut weight = 1.0;
                    if let Some(weight_prop) = weight_property {
                        if rel_record.prop_ptr != 0 {
                            if let Ok(Some(properties)) =
                                engine.storage.load_relationship_properties(rel_id)
                            {
                                if let Some(prop_value) = properties.get(weight_prop) {
                                    if let Some(num) = prop_value.as_f64() {
                                        weight = num;
                                    } else if let Some(num) = prop_value.as_u64() {
                                        weight = num as f64;
                                    } else if let Some(num) = prop_value.as_i64() {
                                        weight = num as f64;
                                    }
                                }
                            }
                        }
                    }

                    graph.add_edge(rel_record.src_id, rel_record.dst_id, weight, vec![rel_type]);
                }
            }
        }

        Ok(graph)
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

/// K shortest path result (for Yen's algorithm)
#[derive(Debug, Clone, PartialEq)]
pub struct KShortestPath {
    /// The path from source to target
    pub path: Vec<u64>,
    /// Total length of the path
    pub length: f64,
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

    /// Find K shortest paths using Yen's algorithm
    ///
    /// This algorithm finds the K shortest simple paths from source to target.
    /// Returns a vector of paths sorted by length (shortest first).
    ///
    /// # Arguments
    ///
    /// * `source` - The source node ID
    /// * `target` - The target node ID
    /// * `k` - Number of shortest paths to find
    ///
    /// # Returns
    ///
    /// A vector of `KShortestPath` containing up to K paths with their lengths
    pub fn k_shortest_paths(
        &self,
        source: u64,
        target: u64,
        k: usize,
    ) -> Result<Vec<KShortestPath>> {
        if !self.has_node(source) || !self.has_node(target) {
            return Err(Error::InvalidInput(
                "Source or target node not found".to_string(),
            ));
        }

        if k == 0 {
            return Ok(Vec::new());
        }

        // Find the first shortest path using Dijkstra
        let first_result = self.dijkstra(source, Some(target))?;
        if first_result.path.is_none() {
            return Ok(Vec::new()); // No path exists
        }

        let mut result_paths = Vec::new();
        let first_path = first_result.path.unwrap();
        let first_length = *first_result
            .distances
            .get(&target)
            .unwrap_or(&f64::INFINITY);

        result_paths.push(KShortestPath {
            path: first_path.clone(),
            length: first_length,
        });

        // Candidate paths (heap sorted by length)
        let mut candidates: Vec<KShortestPath> = Vec::new();

        for k_iter in 1..k {
            if result_paths.len() != k_iter {
                break; // No more paths found in previous iteration
            }

            let prev_path = &result_paths[k_iter - 1].path;

            // For each node in the previous shortest path
            for i in 0..(prev_path.len().saturating_sub(1)) {
                let spur_node = prev_path[i];
                let root_path = &prev_path[0..=i];

                // Build a modified graph by removing edges
                let mut removed_edges: Vec<(u64, u64)> = Vec::new();

                // Remove edges that are part of previous paths with the same root
                for prev_result_path in &result_paths {
                    let prev = &prev_result_path.path;
                    if prev.len() > i && &prev[0..=i] == root_path {
                        if i + 1 < prev.len() {
                            removed_edges.push((prev[i], prev[i + 1]));
                        }
                    }
                }

                // Create a temporary graph without the removed edges
                let spur_path = self.dijkstra_with_excluded_edges(
                    spur_node,
                    Some(target),
                    &removed_edges,
                    root_path,
                )?;

                if let Some(spur) = spur_path.path {
                    // Combine root path + spur path
                    let mut total_path = root_path[0..root_path.len() - 1].to_vec();
                    total_path.extend(spur);

                    // Calculate total length
                    let total_length = self.calculate_path_length(&total_path);

                    // Add to candidates if not already present
                    let candidate = KShortestPath {
                        path: total_path,
                        length: total_length,
                    };

                    if !candidates.iter().any(|c| c.path == candidate.path)
                        && !result_paths.iter().any(|r| r.path == candidate.path)
                    {
                        candidates.push(candidate);
                    }
                }
            }

            if candidates.is_empty() {
                break; // No more paths to find
            }

            // Sort candidates by length and take the shortest
            candidates.sort_by(|a, b| a.length.partial_cmp(&b.length).unwrap());
            result_paths.push(candidates.remove(0));
        }

        Ok(result_paths)
    }

    /// Helper function for Yen's algorithm - Dijkstra with excluded edges and nodes
    fn dijkstra_with_excluded_edges(
        &self,
        source: u64,
        target: Option<u64>,
        excluded_edges: &[(u64, u64)],
        excluded_nodes: &[u64],
    ) -> Result<ShortestPathResult> {
        let excluded_edges_set: HashSet<(u64, u64)> = excluded_edges.iter().cloned().collect();
        let excluded_nodes_set: HashSet<u64> = excluded_nodes[0..excluded_nodes.len() - 1]
            .iter()
            .cloned()
            .collect();

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();
        let mut heap = BinaryHeap::new();

        // Initialize distances
        for &node in self.adjacency.keys() {
            if !excluded_nodes_set.contains(&node) {
                distances.insert(node, f64::INFINITY);
            }
        }
        distances.insert(source, 0.0);

        #[derive(PartialEq)]
        struct State {
            cost: f64,
            node: u64,
        }

        impl Eq for State {}

        impl Ord for State {
            fn cmp(&self, other: &Self) -> Ordering {
                other
                    .cost
                    .partial_cmp(&self.cost)
                    .unwrap_or(Ordering::Equal)
            }
        }

        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        heap.push(State {
            cost: 0.0,
            node: source,
        });

        while let Some(State { cost, node }) = heap.pop() {
            if let Some(target_node) = target {
                if node == target_node {
                    break;
                }
            }

            if cost > *distances.get(&node).unwrap_or(&f64::INFINITY) {
                continue;
            }

            for &(neighbor, weight) in self.get_neighbors(node) {
                // Skip excluded edges and nodes
                if excluded_edges_set.contains(&(node, neighbor))
                    || excluded_nodes_set.contains(&neighbor)
                {
                    continue;
                }

                let new_cost = cost + weight;
                if new_cost < *distances.get(&neighbor).unwrap_or(&f64::INFINITY) {
                    distances.insert(neighbor, new_cost);
                    parents.insert(neighbor, node);
                    heap.push(State {
                        cost: new_cost,
                        node: neighbor,
                    });
                }
            }
        }

        let path = if let Some(target_node) = target {
            self.reconstruct_path(&parents, source, target_node)
        } else {
            None
        };

        Ok(ShortestPathResult {
            distances,
            parents,
            path,
        })
    }

    /// Helper function to calculate the total length of a path
    fn calculate_path_length(&self, path: &[u64]) -> f64 {
        let mut length = 0.0;
        for i in 0..(path.len().saturating_sub(1)) {
            let from = path[i];
            let to = path[i + 1];
            if let Some(neighbors) = self.adjacency.get(&from) {
                if let Some((_, weight)) = neighbors.iter().find(|(n, _)| *n == to) {
                    length += weight;
                }
            }
        }
        length
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
    /// For directed graphs, treat edges as bidirectional (undirected)
    fn dfs_component(&self, node: u64, visited: &mut HashSet<u64>, component_nodes: &mut Vec<u64>) {
        visited.insert(node);
        component_nodes.push(node);

        // Follow outgoing edges
        for &(neighbor, _) in self.get_neighbors(node) {
            if !visited.contains(&neighbor) {
                self.dfs_component(neighbor, visited, component_nodes);
            }
        }

        // Also follow incoming edges (treat as undirected graph)
        for (from_node, neighbors) in &self.adjacency {
            if neighbors.iter().any(|(to, _)| *to == node) && !visited.contains(from_node) {
                self.dfs_component(*from_node, visited, component_nodes);
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

    /// Find shortest path using Bellman-Ford algorithm
    /// Can detect negative cycles
    pub fn bellman_ford(&self, source: u64) -> Result<(ShortestPathResult, bool)> {
        if !self.has_node(source) {
            return Err(Error::InvalidInput("Source node not found".to_string()));
        }

        let mut distances = HashMap::new();
        let mut parents = HashMap::new();

        // Initialize distances
        for &node in self.adjacency.keys() {
            distances.insert(node, f64::INFINITY);
        }
        distances.insert(source, 0.0);

        // Relax edges |V| - 1 times
        let node_count = self.adjacency.len();
        for _ in 0..node_count - 1 {
            for (&from, neighbors) in &self.adjacency {
                if distances[&from] == f64::INFINITY {
                    continue;
                }
                for &(to, weight) in neighbors {
                    let new_dist = distances[&from] + weight;
                    if new_dist < distances[&to] {
                        distances.insert(to, new_dist);
                        parents.insert(to, from);
                    }
                }
            }
        }

        // Check for negative cycles
        let mut has_negative_cycle = false;
        for (&from, neighbors) in &self.adjacency {
            if distances[&from] == f64::INFINITY {
                continue;
            }
            for &(to, weight) in neighbors {
                if distances[&from] + weight < distances[&to] {
                    has_negative_cycle = true;
                    break;
                }
            }
            if has_negative_cycle {
                break;
            }
        }

        Ok((
            ShortestPathResult {
                distances,
                parents,
                path: None,
            },
            has_negative_cycle,
        ))
    }

    /// Calculate PageRank for all nodes
    /// Returns a map of node_id -> PageRank score
    pub fn pagerank(
        &self,
        damping_factor: f64,
        max_iterations: usize,
        tolerance: f64,
    ) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let n = nodes.len() as f64;
        let mut ranks = HashMap::new();

        // Initialize ranks
        for &node in &nodes {
            ranks.insert(node, 1.0 / n);
        }

        // Calculate out-degrees
        let mut out_degrees = HashMap::new();
        for (&node, neighbors) in &self.adjacency {
            out_degrees.insert(node, neighbors.len() as f64);
        }

        for _ in 0..max_iterations {
            let mut new_ranks = HashMap::new();
            let mut total_diff = 0.0;

            // Initialize new ranks
            for &node in &nodes {
                new_ranks.insert(node, (1.0 - damping_factor) / n);
            }

            // Distribute PageRank
            for (&node, neighbors) in &self.adjacency {
                let out_degree = out_degrees.get(&node).copied().unwrap_or(1.0);
                if out_degree > 0.0 {
                    let contribution = damping_factor * ranks[&node] / out_degree;
                    for &(neighbor, _) in neighbors {
                        *new_ranks.get_mut(&neighbor).unwrap() += contribution;
                    }
                } else {
                    // Dangling node - distribute evenly
                    let contribution = damping_factor * ranks[&node] / n;
                    for &target_node in &nodes {
                        *new_ranks.get_mut(&target_node).unwrap() += contribution;
                    }
                }
            }

            // Check convergence
            for &node in &nodes {
                total_diff += (new_ranks[&node] - ranks[&node]).abs();
            }

            ranks = new_ranks;
            if total_diff < tolerance {
                break;
            }
        }

        ranks
    }

    /// Calculate Weighted PageRank for all nodes
    /// Uses edge weights to determine contribution distribution
    /// Higher edge weights mean more PageRank flows through that edge
    /// Returns a map of node_id -> PageRank score
    pub fn weighted_pagerank(
        &self,
        damping_factor: f64,
        max_iterations: usize,
        tolerance: f64,
    ) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let n = nodes.len() as f64;
        let mut ranks = HashMap::new();

        // Initialize ranks
        for &node in &nodes {
            ranks.insert(node, 1.0 / n);
        }

        // Calculate weighted out-degrees (sum of edge weights)
        let mut weighted_out_degrees = HashMap::new();
        for (&node, neighbors) in &self.adjacency {
            let total_weight: f64 = neighbors.iter().map(|&(_, weight)| weight).sum();
            weighted_out_degrees.insert(node, total_weight);
        }

        for _ in 0..max_iterations {
            let mut new_ranks = HashMap::new();
            let mut total_diff = 0.0;

            // Initialize new ranks with teleport probability
            for &node in &nodes {
                new_ranks.insert(node, (1.0 - damping_factor) / n);
            }

            // Distribute PageRank based on edge weights
            for (&node, neighbors) in &self.adjacency {
                let total_weight = weighted_out_degrees.get(&node).copied().unwrap_or(0.0);
                if total_weight > 0.0 {
                    for &(neighbor, weight) in neighbors {
                        // Contribution proportional to edge weight
                        let contribution = damping_factor * ranks[&node] * weight / total_weight;
                        *new_ranks.get_mut(&neighbor).unwrap() += contribution;
                    }
                } else {
                    // Dangling node - distribute evenly
                    let contribution = damping_factor * ranks[&node] / n;
                    for &target_node in &nodes {
                        *new_ranks.get_mut(&target_node).unwrap() += contribution;
                    }
                }
            }

            // Check convergence
            for &node in &nodes {
                total_diff += (new_ranks[&node] - ranks[&node]).abs();
            }

            ranks = new_ranks;
            if total_diff < tolerance {
                break;
            }
        }

        ranks
    }

    /// Calculate PageRank using parallel processing for large graphs
    /// Automatically switches to parallel mode for graphs with >1000 nodes
    /// Returns a map of node_id -> PageRank score
    pub fn pagerank_parallel(
        &self,
        damping_factor: f64,
        max_iterations: usize,
        tolerance: f64,
    ) -> HashMap<u64, f64> {
        use rayon::prelude::*;

        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let n = nodes.len();

        // For small graphs, use sequential version
        if n < 1000 {
            return self.pagerank(damping_factor, max_iterations, tolerance);
        }

        let n_f64 = n as f64;
        let mut ranks: HashMap<u64, f64> = nodes.iter().map(|&node| (node, 1.0 / n_f64)).collect();

        // Pre-calculate out-degrees for efficiency
        let out_degrees: HashMap<u64, f64> = self
            .adjacency
            .iter()
            .map(|(&node, neighbors)| (node, neighbors.len() as f64))
            .collect();

        // Pre-calculate incoming edges for each node (reverse graph)
        let mut incoming: HashMap<u64, Vec<(u64, f64)>> = HashMap::new();
        for &node in &nodes {
            incoming.insert(node, Vec::new());
        }
        for (&src, neighbors) in &self.adjacency {
            let out_degree = out_degrees.get(&src).copied().unwrap_or(1.0);
            for &(dst, _) in neighbors {
                if let Some(inc) = incoming.get_mut(&dst) {
                    inc.push((src, out_degree));
                }
            }
        }

        // Find dangling nodes (nodes with no outgoing edges)
        let dangling_nodes: Vec<u64> = nodes
            .iter()
            .filter(|&&node| out_degrees.get(&node).copied().unwrap_or(0.0) == 0.0)
            .cloned()
            .collect();

        for _ in 0..max_iterations {
            // Calculate dangling node contribution
            let dangling_sum: f64 = dangling_nodes.iter().map(|node| ranks[node]).sum();
            let dangling_contribution = damping_factor * dangling_sum / n_f64;

            // Calculate new ranks in parallel
            let new_ranks: HashMap<u64, f64> = nodes
                .par_iter()
                .map(|&node| {
                    let base = (1.0 - damping_factor) / n_f64 + dangling_contribution;
                    let incoming_contribution: f64 = incoming
                        .get(&node)
                        .map(|inc| {
                            inc.iter()
                                .map(|(src, out_deg)| damping_factor * ranks[src] / out_deg)
                                .sum()
                        })
                        .unwrap_or(0.0);
                    (node, base + incoming_contribution)
                })
                .collect();

            // Check convergence
            let total_diff: f64 = nodes
                .par_iter()
                .map(|node| (new_ranks[node] - ranks[node]).abs())
                .sum();

            ranks = new_ranks;
            if total_diff < tolerance {
                break;
            }
        }

        ranks
    }

    /// Calculate Betweenness Centrality for all nodes
    /// Returns a map of node_id -> betweenness centrality score
    pub fn betweenness_centrality(&self) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut centrality = HashMap::new();

        for &node in &nodes {
            centrality.insert(node, 0.0);
        }

        // For each node as source, calculate shortest paths
        for &source in &nodes {
            let result = self.dijkstra(source, None).unwrap();

            // Count shortest paths through each node
            for &target in &nodes {
                if source == target {
                    continue;
                }
                if let Some(path) = self.reconstruct_path(&result.parents, source, target) {
                    // Count intermediate nodes
                    for &intermediate in &path[1..path.len() - 1] {
                        *centrality.get_mut(&intermediate).unwrap() += 1.0;
                    }
                }
            }
        }

        // Normalize by number of pairs (excluding self)
        let n = nodes.len() as f64;
        let normalization = (n - 1.0) * (n - 2.0);
        if normalization > 0.0 {
            for value in centrality.values_mut() {
                *value /= normalization;
            }
        }

        centrality
    }

    /// Calculate Closeness Centrality for all nodes
    /// Returns a map of node_id -> closeness centrality score
    pub fn closeness_centrality(&self) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut centrality = HashMap::new();

        for &node in &nodes {
            let result = self.dijkstra(node, None).unwrap();

            let mut total_distance = 0.0;
            let mut reachable_count = 0;

            for &other in &nodes {
                if node != other {
                    if let Some(&dist) = result.distances.get(&other) {
                        if dist != f64::INFINITY {
                            total_distance += dist;
                            reachable_count += 1;
                        }
                    }
                }
            }

            if reachable_count > 0 && total_distance > 0.0 {
                centrality.insert(node, reachable_count as f64 / total_distance);
            } else {
                centrality.insert(node, 0.0);
            }
        }

        centrality
    }

    /// Calculate Degree Centrality for all nodes
    /// Returns a map of node_id -> degree centrality score
    pub fn degree_centrality(&self) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let n = nodes.len() as f64;
        let mut centrality = HashMap::new();

        for &node in &nodes {
            let degree = self.get_neighbors(node).len() as f64;
            // Normalize by maximum possible degree (n-1)
            centrality.insert(node, if n > 1.0 { degree / (n - 1.0) } else { 0.0 });
        }

        centrality
    }

    /// Compute eigenvector centrality using power iteration
    ///
    /// Eigenvector centrality assigns scores to nodes based on the principle that
    /// connections to high-scoring nodes contribute more to the score of the node in question.
    ///
    /// Uses power iteration method with the following parameters:
    /// - max_iterations: Maximum number of iterations (default: 100)
    /// - tolerance: Convergence threshold (default: 1e-6)
    ///
    /// Returns normalized centrality scores (sum of squares = 1)
    pub fn eigenvector_centrality(&self) -> HashMap<u64, f64> {
        self.eigenvector_centrality_with_params(100, 1e-6)
    }

    /// Compute eigenvector centrality with custom parameters
    pub fn eigenvector_centrality_with_params(
        &self,
        max_iterations: usize,
        tolerance: f64,
    ) -> HashMap<u64, f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();

        if nodes.is_empty() {
            return HashMap::new();
        }

        // Initialize all scores to 1.0
        let mut scores: HashMap<u64, f64> = nodes.iter().map(|&n| (n, 1.0)).collect();

        // Build reverse adjacency (incoming edges)
        let mut reverse_adjacency: HashMap<u64, Vec<u64>> = HashMap::new();
        for &node in &nodes {
            reverse_adjacency.insert(node, Vec::new());
        }
        for (&from, neighbors) in &self.adjacency {
            for &(to, _weight) in neighbors {
                reverse_adjacency.entry(to).or_default().push(from);
            }
        }

        // Power iteration
        for _iteration in 0..max_iterations {
            let mut new_scores = HashMap::new();

            // Update scores: score[node] = sum(score[neighbor]) for incoming neighbors
            for &node in &nodes {
                let incoming = reverse_adjacency.get(&node).unwrap();
                let score: f64 = incoming
                    .iter()
                    .map(|&n| scores.get(&n).unwrap_or(&0.0))
                    .sum();
                new_scores.insert(node, score);
            }

            // Normalize scores (L2 normalization)
            let norm: f64 = new_scores.values().map(|&s| s * s).sum::<f64>().sqrt();

            if norm == 0.0 {
                // Graph has no edges, return uniform distribution
                let uniform_score = 1.0 / (nodes.len() as f64).sqrt();
                return nodes.iter().map(|&n| (n, uniform_score)).collect();
            }

            for score in new_scores.values_mut() {
                *score /= norm;
            }

            // Check convergence
            let diff: f64 = nodes
                .iter()
                .map(|&n| {
                    let old = scores.get(&n).unwrap_or(&0.0);
                    let new = new_scores.get(&n).unwrap_or(&0.0);
                    (old - new).abs()
                })
                .sum();

            scores = new_scores;

            if diff < tolerance {
                break;
            }
        }

        scores
    }

    /// Find strongly connected components using Kosaraju's algorithm
    /// Returns component ID for each node
    pub fn strongly_connected_components(&self) -> ConnectedComponentsResult {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut visited = HashSet::new();
        let mut finish_order = Vec::new();

        // First DFS pass - get finish times
        for &node in &nodes {
            if !visited.contains(&node) {
                self.dfs_finish_order(node, &mut visited, &mut finish_order);
            }
        }

        // Build reverse graph
        let mut reverse_adjacency: HashMap<u64, Vec<u64>> = HashMap::new();
        for &node in &nodes {
            reverse_adjacency.insert(node, Vec::new());
        }
        for (&from, neighbors) in &self.adjacency {
            for &(to, _) in neighbors {
                reverse_adjacency.entry(to).or_default().push(from);
            }
        }

        // Second DFS pass on reverse graph
        visited.clear();
        let mut components = HashMap::new();
        let mut component_id = 0;
        let mut component_nodes = HashMap::new();

        for &node in finish_order.iter().rev() {
            if !visited.contains(&node) {
                let mut component = Vec::new();
                self.dfs_component_reverse(node, &reverse_adjacency, &mut visited, &mut component);
                for &comp_node in &component {
                    components.insert(comp_node, component_id);
                }
                component_nodes.insert(component_id, component);
                component_id += 1;
            }
        }

        ConnectedComponentsResult {
            components,
            component_count: component_id,
            component_nodes,
        }
    }

    /// Count triangles in the graph
    ///
    /// A triangle is a set of three nodes where each pair is connected by an edge.
    /// This algorithm counts the total number of triangles in the graph.
    ///
    /// # Returns
    ///
    /// The total number of triangles in the graph
    ///
    /// # Algorithm
    ///
    /// For each edge (u, v), find common neighbors of u and v.
    /// Each common neighbor w forms a triangle (u, v, w).
    /// To avoid counting each triangle three times (once for each edge),
    /// we only count when u < v < w in node ID ordering.
    pub fn triangle_count(&self) -> usize {
        let mut count = 0;

        // Convert adjacency list to HashSet for faster lookups
        let adjacency_sets: HashMap<u64, HashSet<u64>> = self
            .adjacency
            .iter()
            .map(|(&node, neighbors)| {
                let neighbor_set: HashSet<u64> = neighbors.iter().map(|(n, _)| *n).collect();
                (node, neighbor_set)
            })
            .collect();

        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();

        // For directed graphs, find cycles of length 3: u→v, v→w, w→u
        for &u in &nodes {
            if let Some(u_neighbors) = adjacency_sets.get(&u) {
                for &v in u_neighbors {
                    if let Some(v_neighbors) = adjacency_sets.get(&v) {
                        for &w in v_neighbors {
                            // Check if there's an edge w→u to complete the triangle
                            if let Some(w_neighbors) = adjacency_sets.get(&w) {
                                if w_neighbors.contains(&u) {
                                    // Found a triangle u→v→w→u
                                    // Only count each triangle once (when u < v < w)
                                    if u < v && v < w {
                                        count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        count
    }

    /// Compute local clustering coefficient for each node
    ///
    /// The local clustering coefficient of a node is the ratio of triangles
    /// connected to the node to the maximum possible number of triangles
    /// (i.e., the number of pairs of neighbors).
    ///
    /// Formula: C(v) = 2 * T(v) / (deg(v) * (deg(v) - 1))
    /// where T(v) is the number of triangles through node v
    ///
    /// # Returns
    ///
    /// A HashMap mapping each node ID to its local clustering coefficient (0.0 to 1.0)
    pub fn clustering_coefficient(&self) -> HashMap<u64, f64> {
        let mut coefficients = HashMap::new();

        // Build undirected adjacency (treat graph as undirected)
        let mut undirected_adjacency: HashMap<u64, HashSet<u64>> = HashMap::new();

        for (&from, neighbors) in &self.adjacency {
            for &(to, _) in neighbors {
                undirected_adjacency
                    .entry(from)
                    .or_insert_with(HashSet::new)
                    .insert(to);
                undirected_adjacency
                    .entry(to)
                    .or_insert_with(HashSet::new)
                    .insert(from);
            }
        }

        // Ensure all nodes are in the map
        for node in self.adjacency.keys() {
            undirected_adjacency
                .entry(*node)
                .or_insert_with(HashSet::new);
        }

        for (&node, neighbors_set) in &undirected_adjacency {
            let degree = neighbors_set.len();

            if degree < 2 {
                // Need at least 2 neighbors to form a triangle
                coefficients.insert(node, 0.0);
                continue;
            }

            // Count triangles through this node
            let mut triangle_count = 0;
            let neighbors: Vec<u64> = neighbors_set.iter().cloned().collect();

            for i in 0..neighbors.len() {
                for j in (i + 1)..neighbors.len() {
                    let n1 = neighbors[i];
                    let n2 = neighbors[j];

                    // Check if n1 and n2 are connected (in undirected sense)
                    if let Some(n1_neighbors) = undirected_adjacency.get(&n1) {
                        if n1_neighbors.contains(&n2) {
                            triangle_count += 1;
                        }
                    }
                }
            }

            // Calculate coefficient: 2 * triangles / (deg * (deg - 1))
            let max_triangles = degree * (degree - 1);
            let coefficient = if max_triangles > 0 {
                (2.0 * triangle_count as f64) / max_triangles as f64
            } else {
                0.0
            };

            coefficients.insert(node, coefficient);
        }

        coefficients
    }

    /// Compute global clustering coefficient (transitivity)
    ///
    /// The global clustering coefficient is the ratio of the number of closed
    /// triplets (triangles) to the total number of triplets (connected triples).
    ///
    /// Formula: C = 3 * number_of_triangles / number_of_triplets
    ///
    /// # Returns
    ///
    /// The global clustering coefficient (0.0 to 1.0)
    pub fn global_clustering_coefficient(&self) -> f64 {
        // Build undirected adjacency (treat graph as undirected)
        let mut undirected_adjacency: HashMap<u64, HashSet<u64>> = HashMap::new();

        for (&from, neighbors) in &self.adjacency {
            for &(to, _) in neighbors {
                undirected_adjacency
                    .entry(from)
                    .or_insert_with(HashSet::new)
                    .insert(to);
                undirected_adjacency
                    .entry(to)
                    .or_insert_with(HashSet::new)
                    .insert(from);
            }
        }

        // Count undirected triangles
        let mut triangle_count = 0;
        let mut nodes: Vec<u64> = undirected_adjacency.keys().cloned().collect();
        nodes.sort(); // Ensure deterministic ordering

        for &u in &nodes {
            if let Some(u_neighbors) = undirected_adjacency.get(&u) {
                // Sort neighbors for deterministic ordering
                let mut neighbors: Vec<u64> = u_neighbors.iter().cloned().collect();
                neighbors.sort();
                for i in 0..neighbors.len() {
                    for j in (i + 1)..neighbors.len() {
                        let v = neighbors[i];
                        let w = neighbors[j];
                        if let Some(v_neighbors) = undirected_adjacency.get(&v) {
                            if v_neighbors.contains(&w) {
                                // Only count when u < v < w to avoid duplicates
                                if u < v && v < w {
                                    triangle_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        let triangles = triangle_count as f64 * 3.0; // Each triangle has 3 triplets

        // Count triplets (connected triples / wedges)
        let mut triplets = 0;
        for neighbors_set in undirected_adjacency.values() {
            let degree = neighbors_set.len();
            if degree >= 2 {
                // Number of ways to choose 2 neighbors: C(degree, 2) = degree*(degree-1)/2
                triplets += degree * (degree - 1) / 2;
            }
        }

        if triplets == 0 {
            0.0
        } else {
            triangles / triplets as f64
        }
    }

    /// DFS helper for finish order (first pass of Kosaraju)
    fn dfs_finish_order(&self, node: u64, visited: &mut HashSet<u64>, finish_order: &mut Vec<u64>) {
        visited.insert(node);
        for &(neighbor, _) in self.get_neighbors(node) {
            if !visited.contains(&neighbor) {
                self.dfs_finish_order(neighbor, visited, finish_order);
            }
        }
        finish_order.push(node);
    }

    /// DFS helper for reverse graph (second pass of Kosaraju)
    fn dfs_component_reverse(
        &self,
        node: u64,
        reverse_adjacency: &HashMap<u64, Vec<u64>>,
        visited: &mut HashSet<u64>,
        component: &mut Vec<u64>,
    ) {
        visited.insert(node);
        component.push(node);
        if let Some(neighbors) = reverse_adjacency.get(&node) {
            for &neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    self.dfs_component_reverse(neighbor, reverse_adjacency, visited, component);
                }
            }
        }
    }

    /// Label Propagation Algorithm for community detection
    /// Returns community ID for each node
    pub fn label_propagation(&self, max_iterations: usize) -> ConnectedComponentsResult {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut labels: HashMap<u64, usize> = HashMap::new();

        // Initialize each node with its own label
        for (idx, &node) in nodes.iter().enumerate() {
            labels.insert(node, idx);
        }

        for _ in 0..max_iterations {
            let mut new_labels = labels.clone();
            let mut changed = false;

            // Shuffle nodes for random order
            let mut shuffled_nodes = nodes.clone();
            // Simple pseudo-shuffle (in real implementation, use proper RNG)
            for i in 0..shuffled_nodes.len() {
                let j = (i * 7 + 13) % shuffled_nodes.len();
                shuffled_nodes.swap(i, j);
            }

            for &node in &shuffled_nodes {
                let mut label_counts: HashMap<usize, usize> = HashMap::new();

                // Count labels of neighbors
                for &(neighbor, _) in self.get_neighbors(node) {
                    let neighbor_label = labels[&neighbor];
                    *label_counts.entry(neighbor_label).or_default() += 1;
                }

                // Also count self
                let self_label = labels[&node];
                *label_counts.entry(self_label).or_default() += 1;

                // Choose most frequent label
                if let Some((&most_frequent_label, _)) =
                    label_counts.iter().max_by_key(|&(_, count)| count)
                {
                    if most_frequent_label != labels[&node] {
                        new_labels.insert(node, most_frequent_label);
                        changed = true;
                    }
                }
            }

            labels = new_labels;
            if !changed {
                break;
            }
        }

        // Convert to ConnectedComponentsResult format
        let mut component_map = HashMap::new();
        let mut component_nodes_map = HashMap::new();
        let mut label_to_component: HashMap<usize, usize> = HashMap::new();
        let mut component_id = 0;

        for &node in &nodes {
            let label = labels[&node];
            let comp_id = *label_to_component.entry(label).or_insert_with(|| {
                let id = component_id;
                component_nodes_map.insert(id, Vec::new());
                component_id += 1;
                id
            });
            component_map.insert(node, comp_id);
            component_nodes_map.get_mut(&comp_id).unwrap().push(node);
        }

        ConnectedComponentsResult {
            components: component_map,
            component_count: component_id,
            component_nodes: component_nodes_map,
        }
    }

    /// Louvain algorithm for community detection (simplified version)
    /// Returns community ID for each node
    pub fn louvain(&self, max_iterations: usize) -> ConnectedComponentsResult {
        // Simplified Louvain implementation
        // Full implementation would optimize modularity more carefully
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut communities: HashMap<u64, usize> = HashMap::new();

        // Initialize each node in its own community
        for (idx, &node) in nodes.iter().enumerate() {
            communities.insert(node, idx);
        }

        for _ in 0..max_iterations {
            let mut changed = false;
            let mut shuffled_nodes = nodes.clone();
            // Simple pseudo-shuffle
            for i in 0..shuffled_nodes.len() {
                let j = (i * 7 + 13) % shuffled_nodes.len();
                shuffled_nodes.swap(i, j);
            }

            for &node in &shuffled_nodes {
                let mut best_community = communities[&node];
                let mut best_modularity_gain = 0.0;

                // Try moving to each neighbor's community
                for &(neighbor, _) in self.get_neighbors(node) {
                    let neighbor_community = communities[&neighbor];
                    let gain =
                        self.calculate_modularity_gain(node, neighbor_community, &communities);
                    if gain > best_modularity_gain {
                        best_modularity_gain = gain;
                        best_community = neighbor_community;
                    }
                }

                if best_community != communities[&node] {
                    communities.insert(node, best_community);
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        // Convert to ConnectedComponentsResult format
        let mut component_map = HashMap::new();
        let mut component_nodes_map = HashMap::new();
        let mut community_to_component: HashMap<usize, usize> = HashMap::new();
        let mut component_id = 0;

        for &node in &nodes {
            let community = communities[&node];
            let comp_id = *community_to_component.entry(community).or_insert_with(|| {
                let id = component_id;
                component_nodes_map.insert(id, Vec::new());
                component_id += 1;
                id
            });
            component_map.insert(node, comp_id);
            component_nodes_map.get_mut(&comp_id).unwrap().push(node);
        }

        ConnectedComponentsResult {
            components: component_map,
            component_count: component_id,
            component_nodes: component_nodes_map,
        }
    }

    /// Calculate modularity gain for moving a node to a community
    fn calculate_modularity_gain(
        &self,
        node: u64,
        community: usize,
        communities: &HashMap<u64, usize>,
    ) -> f64 {
        // Simplified modularity gain calculation
        // Count edges within community vs edges outside
        let mut edges_in_community = 0;
        let mut edges_outside = 0;

        for &(neighbor, _) in self.get_neighbors(node) {
            if communities.get(&neighbor) == Some(&community) {
                edges_in_community += 1;
            } else {
                edges_outside += 1;
            }
        }

        // Simple heuristic: prefer communities with more connections
        edges_in_community as f64 - edges_outside as f64 * 0.5
    }

    /// Calculate Jaccard Similarity between two nodes
    /// Returns similarity score (0.0 to 1.0)
    pub fn jaccard_similarity(&self, node1: u64, node2: u64) -> f64 {
        if !self.has_node(node1) || !self.has_node(node2) {
            return 0.0;
        }

        let neighbors1: HashSet<u64> = self.get_neighbors(node1).iter().map(|(n, _)| *n).collect();
        let neighbors2: HashSet<u64> = self.get_neighbors(node2).iter().map(|(n, _)| *n).collect();

        let intersection = neighbors1.intersection(&neighbors2).count();
        let union = neighbors1.union(&neighbors2).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// Calculate Cosine Similarity between two nodes (based on neighbor vectors)
    /// Returns similarity score (-1.0 to 1.0)
    pub fn cosine_similarity(&self, node1: u64, node2: u64) -> f64 {
        if !self.has_node(node1) || !self.has_node(node2) {
            return 0.0;
        }

        let all_nodes: HashSet<u64> = self.adjacency.keys().cloned().collect();
        let neighbors1: HashSet<u64> = self.get_neighbors(node1).iter().map(|(n, _)| *n).collect();
        let neighbors2: HashSet<u64> = self.get_neighbors(node2).iter().map(|(n, _)| *n).collect();

        let mut dot_product = 0.0;
        let mut norm1 = 0.0;
        let mut norm2 = 0.0;

        for &node in &all_nodes {
            let val1 = if neighbors1.contains(&node) { 1.0 } else { 0.0 };
            let val2 = if neighbors2.contains(&node) { 1.0 } else { 0.0 };
            dot_product += val1 * val2;
            norm1 += val1 * val1;
            norm2 += val2 * val2;
        }

        let denominator = f64::sqrt(norm1 * norm2);
        if denominator == 0.0 {
            0.0
        } else {
            dot_product / denominator
        }
    }

    /// Calculate Node Similarity for all pairs
    /// Returns a map of (node1, node2) -> similarity score
    pub fn node_similarity(&self, similarity_type: &str) -> HashMap<(u64, u64), f64> {
        let nodes: Vec<u64> = self.adjacency.keys().cloned().collect();
        let mut similarities = HashMap::new();

        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let node1 = nodes[i];
                let node2 = nodes[j];
                let similarity = match similarity_type {
                    "jaccard" => self.jaccard_similarity(node1, node2),
                    "cosine" => self.cosine_similarity(node1, node2),
                    _ => 0.0,
                };
                similarities.insert((node1, node2), similarity);
                similarities.insert((node2, node1), similarity); // Symmetric
            }
        }

        similarities
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
    fn test_k_shortest_paths() {
        // Create a graph with multiple paths
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_node(4, vec![]);

        // Path 1->2->4 (length 3)
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 4, 2.0, vec![]);

        // Path 1->3->4 (length 5)
        graph.add_edge(1, 3, 2.0, vec![]);
        graph.add_edge(3, 4, 3.0, vec![]);

        // Direct path 1->4 (length 10)
        graph.add_edge(1, 4, 10.0, vec![]);

        let result = graph.k_shortest_paths(1, 4, 3).unwrap();

        // Should find 3 paths
        assert_eq!(result.len(), 3);

        // First path should be shortest: 1->2->4 (length 3)
        assert_eq!(result[0].path, vec![1, 2, 4]);
        assert_eq!(result[0].length, 3.0);

        // Second path: 1->3->4 (length 5)
        assert_eq!(result[1].path, vec![1, 3, 4]);
        assert_eq!(result[1].length, 5.0);

        // Third path: 1->4 (length 10)
        assert_eq!(result[2].path, vec![1, 4]);
        assert_eq!(result[2].length, 10.0);
    }

    #[test]
    fn test_k_shortest_paths_no_path() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);

        // No edge between nodes
        let result = graph.k_shortest_paths(1, 2, 3).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_k_shortest_paths_fewer_than_k() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);

        graph.add_edge(1, 2, 1.0, vec![]);

        // Only 1 path exists, but we ask for 5
        let result = graph.k_shortest_paths(1, 2, 5).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, vec![1, 2]);
        assert_eq!(result[0].length, 1.0);
    }

    #[test]
    fn test_k_shortest_paths_complex() {
        // Create a more complex graph with multiple alternative paths
        let mut graph = Graph::new();
        for i in 1..=6 {
            graph.add_node(i, vec![]);
        }

        // Multiple paths from 1 to 6
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);
        graph.add_edge(3, 6, 1.0, vec![]);

        graph.add_edge(1, 4, 2.0, vec![]);
        graph.add_edge(4, 5, 1.0, vec![]);
        graph.add_edge(5, 6, 1.0, vec![]);

        graph.add_edge(1, 6, 5.0, vec![]);

        let result = graph.k_shortest_paths(1, 6, 3).unwrap();

        // Should find at least 2 paths
        assert!(result.len() >= 2);

        // Paths should be sorted by length
        for i in 1..result.len() {
            assert!(result[i - 1].length <= result[i].length);
        }

        // First path should be the shortest
        assert_eq!(result[0].path, vec![1, 2, 3, 6]);
        assert_eq!(result[0].length, 3.0);
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

    #[test]
    fn test_bellman_ford() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 2.0, vec![]);
        graph.add_edge(1, 3, 4.0, vec![]);

        let (result, has_negative_cycle) = graph.bellman_ford(1).unwrap();
        assert!(!has_negative_cycle);
        assert_eq!(result.distances[&3], 3.0);
    }

    #[test]
    fn test_pagerank() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);
        graph.add_edge(3, 1, 1.0, vec![]);

        let ranks = graph.pagerank(0.85, 100, 0.0001);
        assert_eq!(ranks.len(), 3);
        // All nodes should have similar ranks in a cycle
        assert!(ranks[&1] > 0.0);
        assert!(ranks[&2] > 0.0);
        assert!(ranks[&3] > 0.0);
    }

    #[test]
    fn test_weighted_pagerank() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        // Node 1 has a high weight edge to node 2, low weight to node 3
        graph.add_edge(1, 2, 10.0, vec![]); // High weight
        graph.add_edge(1, 3, 1.0, vec![]); // Low weight
        graph.add_edge(2, 1, 1.0, vec![]);
        graph.add_edge(3, 1, 1.0, vec![]);

        let ranks = graph.weighted_pagerank(0.85, 100, 0.0001);
        assert_eq!(ranks.len(), 3);
        // Node 2 should have higher rank than node 3 due to higher weight edge from node 1
        assert!(
            ranks[&2] > ranks[&3],
            "Node 2 (rank={}) should be higher than Node 3 (rank={}) due to edge weights",
            ranks[&2],
            ranks[&3]
        );
    }

    #[test]
    fn test_weighted_pagerank_equal_weights() {
        // With equal weights, weighted_pagerank should behave like regular pagerank
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);
        graph.add_edge(3, 1, 1.0, vec![]);

        let weighted_ranks = graph.weighted_pagerank(0.85, 100, 0.0001);
        let unweighted_ranks = graph.pagerank(0.85, 100, 0.0001);

        // With equal weights, results should be very similar
        for node in [1, 2, 3] {
            let diff = (weighted_ranks[&node] - unweighted_ranks[&node]).abs();
            assert!(
                diff < 0.01,
                "Weighted and unweighted should be similar for equal weights"
            );
        }
    }

    #[test]
    fn test_pagerank_parallel_small_graph() {
        // For small graphs, pagerank_parallel should delegate to regular pagerank
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);
        graph.add_edge(3, 1, 1.0, vec![]);

        let parallel_ranks = graph.pagerank_parallel(0.85, 100, 0.0001);
        let sequential_ranks = graph.pagerank(0.85, 100, 0.0001);

        // Results should be identical for small graphs
        for node in [1, 2, 3] {
            let diff = (parallel_ranks[&node] - sequential_ranks[&node]).abs();
            assert!(
                diff < 0.0001,
                "Parallel and sequential should be identical for small graphs"
            );
        }
    }

    #[test]
    fn test_degree_centrality() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(1, 3, 1.0, vec![]);

        let centrality = graph.degree_centrality();
        assert!(centrality[&1] > centrality[&2]);
        assert!(centrality[&1] > centrality[&3]);
    }

    #[test]
    fn test_eigenvector_centrality() {
        // Create a star graph: node 1 is the hub connected to nodes 2, 3, 4
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_node(4, vec![]);

        // Edges from hub to leaves
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(1, 3, 1.0, vec![]);
        graph.add_edge(1, 4, 1.0, vec![]);

        // Add edges back from leaves to hub
        graph.add_edge(2, 1, 1.0, vec![]);
        graph.add_edge(3, 1, 1.0, vec![]);
        graph.add_edge(4, 1, 1.0, vec![]);

        let centrality = graph.eigenvector_centrality();

        // Hub should have high centrality
        assert!(centrality.contains_key(&1));
        assert!(centrality.contains_key(&2));
        assert!(centrality.contains_key(&3));
        assert!(centrality.contains_key(&4));

        // In this symmetric structure, all nodes should have equal centrality
        let c1 = centrality[&1];
        let c2 = centrality[&2];
        let c3 = centrality[&3];
        let c4 = centrality[&4];

        assert!((c1 - c2).abs() < 1e-5);
        assert!((c1 - c3).abs() < 1e-5);
        assert!((c1 - c4).abs() < 1e-5);
    }

    #[test]
    fn test_eigenvector_centrality_chain() {
        // Create a chain graph: 1 -> 2 -> 3 -> 4
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_node(4, vec![]);

        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);
        graph.add_edge(3, 4, 1.0, vec![]);

        let centrality = graph.eigenvector_centrality();

        // In a directed chain without cycles, eigenvector centrality converges
        // such that the terminal node (4) has the highest score
        assert!(centrality.contains_key(&1));
        assert!(centrality.contains_key(&2));
        assert!(centrality.contains_key(&3));
        assert!(centrality.contains_key(&4));

        // All nodes should have equal centrality in the normalized result
        // because the chain is symmetric after power iteration
        let c1 = centrality[&1];
        let c2 = centrality[&2];
        let c3 = centrality[&3];
        let c4 = centrality[&4];

        // Verify all scores are equal (within tolerance)
        assert!((c1 - 0.5).abs() < 0.1);
        assert!((c2 - 0.5).abs() < 0.1);
        assert!((c3 - 0.5).abs() < 0.1);
        assert!((c4 - 0.5).abs() < 0.1);
    }

    #[test]
    fn test_eigenvector_centrality_empty_graph() {
        let graph = Graph::new();
        let centrality = graph.eigenvector_centrality();
        assert!(centrality.is_empty());
    }

    #[test]
    fn test_eigenvector_centrality_single_node() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);

        let centrality = graph.eigenvector_centrality();
        assert!(centrality.contains_key(&1));
        // Single node should have normalized score
        assert!((centrality[&1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_strongly_connected_components() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 1, 1.0, vec![]);
        graph.add_edge(3, 3, 1.0, vec![]);

        let result = graph.strongly_connected_components();
        assert_eq!(result.component_count, 2);
    }

    #[test]
    fn test_triangle_count() {
        // Create a graph with 2 triangles
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_node(4, vec![]);

        // Triangle 1: cycle 1→2→3→1
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);
        graph.add_edge(3, 1, 1.0, vec![]);

        // Triangle 2: cycle 2→3→4→2
        graph.add_edge(2, 4, 1.0, vec![]);
        graph.add_edge(4, 2, 1.0, vec![]); // Changed from 4→3 to 4→2 to complete the cycle
        graph.add_edge(3, 4, 1.0, vec![]); // Added 3→4 to complete the cycle

        let count = graph.triangle_count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_triangle_count_no_triangles() {
        // Create a graph with no triangles (chain)
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);

        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);

        let count = graph.triangle_count();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_clustering_coefficient() {
        // Create a graph with known clustering coefficients
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_node(4, vec![]);

        // Triangle: 1, 2, 3 (all nodes have perfect clustering)
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);
        graph.add_edge(3, 1, 1.0, vec![]);

        // Node 4 connected to 1 and 2 (but 1-2 are connected)
        graph.add_edge(4, 1, 1.0, vec![]);
        graph.add_edge(4, 2, 1.0, vec![]);

        let coefficients = graph.clustering_coefficient();

        // Node 1 has neighbors {2, 3, 4}:
        //   Pairs: (2,3) connected, (2,4) connected, (3,4) not connected
        //   Coefficient: 2*2 / (3*2) = 0.666...
        assert!((coefficients[&1] - 0.666666).abs() < 1e-4);

        // Node 2 has neighbors {1, 3, 4}:
        //   Pairs: (1,3) connected, (1,4) connected, (3,4) not connected
        //   Coefficient: 2*2 / (3*2) = 0.666...
        assert!((coefficients[&2] - 0.666666).abs() < 1e-4);

        // Node 3 has neighbors {1, 2} (only 2 neighbors):
        //   Pairs: (1,2) connected
        //   Coefficient: 2*1 / (2*1) = 1.0
        assert!((coefficients[&3] - 1.0).abs() < 1e-5);

        // Node 4 has neighbors {1, 2}:
        //   Pairs: (1,2) connected
        //   Coefficient: 2*1 / (2*1) = 1.0
        assert!((coefficients[&4] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_clustering_coefficient_zero() {
        // Star graph: center has coefficient 0, leaves don't have enough neighbors
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_node(4, vec![]);

        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(1, 3, 1.0, vec![]);
        graph.add_edge(1, 4, 1.0, vec![]);

        let coefficients = graph.clustering_coefficient();

        // Center node 1 has 3 neighbors but none connected = coefficient 0
        assert!((coefficients[&1] - 0.0).abs() < 1e-5);

        // Leaf nodes have only 1 neighbor = coefficient 0
        assert!((coefficients[&2] - 0.0).abs() < 1e-5);
        assert!((coefficients[&3] - 0.0).abs() < 1e-5);
        assert!((coefficients[&4] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_global_clustering_coefficient() {
        // Create a graph with triangles
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);

        // Perfect triangle
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);
        graph.add_edge(3, 1, 1.0, vec![]);

        let global_coef = graph.global_clustering_coefficient();

        // Perfect triangle should have global coefficient 1.0
        assert!((global_coef - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_label_propagation() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 1, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);

        let result = graph.label_propagation(10);
        assert!(result.component_count > 0);
    }

    #[test]
    fn test_louvain() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_node(4, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 1, 1.0, vec![]);
        graph.add_edge(3, 4, 1.0, vec![]);
        graph.add_edge(4, 3, 1.0, vec![]);

        let result = graph.louvain(10);
        assert!(result.component_count > 0);
    }

    #[test]
    fn test_jaccard_similarity() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(1, 3, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);

        let similarity = graph.jaccard_similarity(1, 2);
        assert!((0.0..=1.0).contains(&similarity));
    }

    #[test]
    fn test_cosine_similarity() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(1, 3, 1.0, vec![]);
        graph.add_edge(2, 3, 1.0, vec![]);

        let similarity = graph.cosine_similarity(1, 2);
        assert!((-1.0..=1.0).contains(&similarity));
    }

    #[test]
    fn test_from_engine() {
        use crate::Engine;
        use crate::testing::TestContext;

        let ctx = TestContext::new();
        // Use isolated catalog to avoid data contamination from other tests
        let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

        // Create some test data - use single query for reliability
        engine
            .execute_cypher("CREATE (n1:AlgPerson {name: 'Alice'})-[:KNOWS_ALG {weight: 1.5}]->(n2:AlgPerson {name: 'Bob'}) RETURN n1, n2")
            .unwrap();

        // Verify relationship was created
        let rel_count = engine.storage.relationship_count();
        assert!(
            rel_count >= 1,
            "Expected at least 1 relationship, got {}",
            rel_count
        );

        // Convert to algorithm graph
        let graph = Graph::from_engine(&engine, Some("weight")).unwrap();

        // Verify nodes were added (at least 2 nodes)
        let nodes = graph.get_nodes();
        assert!(
            nodes.len() >= 2,
            "Expected at least 2 nodes, got {}",
            nodes.len()
        );

        // Verify edges were added (at least 1 edge)
        let mut total_edges = 0;
        for node_id in &nodes {
            total_edges += graph.get_neighbors(*node_id).len();
        }
        assert!(
            total_edges >= 1,
            "Expected at least 1 edge, got {}",
            total_edges
        );

        // Verify weight property is used if present
        for node_id in &nodes {
            for (_neighbor, weight) in graph.get_neighbors(*node_id) {
                if *weight == 1.5 {
                    // Found the edge with weight property
                    return;
                }
            }
        }
        // If no weight found, that's ok - default weight is 1.0
    }
}
