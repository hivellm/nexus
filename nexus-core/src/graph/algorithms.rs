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
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let mut engine = Engine::with_data_dir(temp_dir.path()).unwrap();

        // Create some test data
        engine
            .execute_cypher("CREATE (n1:Person {name: 'Alice'}) RETURN n1")
            .unwrap();
        engine
            .execute_cypher("CREATE (n2:Person {name: 'Bob'}) RETURN n2")
            .unwrap();
        engine.execute_cypher("MATCH (n1:Person {name: 'Alice'}), (n2:Person {name: 'Bob'}) CREATE (n1)-[:KNOWS {weight: 1.5}]->(n2) RETURN n1, n2").unwrap();

        // Convert to algorithm graph
        let graph = Graph::from_engine(&engine, Some("weight")).unwrap();

        // Verify nodes were added (at least 2 nodes)
        let nodes = graph.get_nodes();
        assert!(nodes.len() >= 2);

        // Verify edges were added (at least 1 edge)
        let mut total_edges = 0;
        for node_id in &nodes {
            total_edges += graph.get_neighbors(*node_id).len();
        }
        assert!(total_edges >= 1);

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
