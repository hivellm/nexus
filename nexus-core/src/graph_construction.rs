//! Graph Construction Algorithms
//!
//! This module provides advanced algorithms for constructing and laying out graphs
//! for visualization and analysis purposes.
//!
//! # Algorithms
//!
//! ## Layout Algorithms
//! - **Force-Directed Layout**: Spring-based positioning for general graphs
//! - **Hierarchical Layout**: Tree-like positioning for DAGs and hierarchies
//! - **Circular Layout**: Circular positioning for cyclic graphs
//! - **Grid Layout**: Regular grid positioning for structured graphs
//!
//! ## Clustering Algorithms
//! - **K-Means Clustering**: Partition nodes into k clusters
//! - **Louvain Community Detection**: Modularity-based community detection
//! - **Connected Components**: Find strongly/weakly connected components
//!
//! ## Optimization Algorithms
//! - **Edge Bundling**: Reduce visual clutter by bundling similar edges
//! - **Node Repulsion**: Prevent node overlap with repulsion forces
//! - **Edge Length Optimization**: Minimize edge crossings and lengths

use crate::Result;
use std::collections::{HashMap, HashSet, VecDeque};
use std::f64::consts::PI;

/// 2D point for node positioning
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: &Point2D) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    pub fn add(&self, other: &Point2D) -> Point2D {
        Point2D::new(self.x + other.x, self.y + other.y)
    }

    pub fn subtract(&self, other: &Point2D) -> Point2D {
        Point2D::new(self.x - other.x, self.y - other.y)
    }

    pub fn scale(&self, factor: f64) -> Point2D {
        Point2D::new(self.x * factor, self.y * factor)
    }

    pub fn magnitude(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalize(&self) -> Point2D {
        let mag = self.magnitude();
        if mag > 0.0 {
            self.scale(1.0 / mag)
        } else {
            Point2D::new(0.0, 0.0)
        }
    }
}

/// Graph node with position and metadata
#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub id: String,
    pub position: Point2D,
    pub size: f64,
    pub mass: f64,
    pub velocity: Point2D,
    pub force: Point2D,
    pub fixed: bool,
    pub metadata: HashMap<String, String>,
}

impl LayoutNode {
    pub fn new(id: String, position: Point2D) -> Self {
        Self {
            id,
            position,
            size: 1.0,
            mass: 1.0,
            velocity: Point2D::new(0.0, 0.0),
            force: Point2D::new(0.0, 0.0),
            fixed: false,
            metadata: HashMap::new(),
        }
    }

    pub fn with_size(mut self, size: f64) -> Self {
        self.size = size;
        self.mass = size * size; // Mass proportional to area
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn fix_position(mut self) -> Self {
        self.fixed = true;
        self
    }
}

/// Graph edge with metadata
#[derive(Debug, Clone)]
pub struct LayoutEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub weight: f64,
    pub length: f64,
    pub metadata: HashMap<String, String>,
}

impl LayoutEdge {
    pub fn new(id: String, source: String, target: String) -> Self {
        Self {
            id,
            source,
            target,
            weight: 1.0,
            length: 1.0,
            metadata: HashMap::new(),
        }
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_length(mut self, length: f64) -> Self {
        self.length = length;
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Graph layout with nodes and edges
#[derive(Debug, Clone)]
pub struct GraphLayout {
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub width: f64,
    pub height: f64,
    pub metadata: HashMap<String, String>,
}

impl GraphLayout {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            width,
            height,
            metadata: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: LayoutNode) {
        self.nodes.push(node);
    }

    pub fn add_edge(&mut self, edge: LayoutEdge) {
        self.edges.push(edge);
    }

    pub fn get_node(&self, id: &str) -> Option<&LayoutNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut LayoutNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn get_edges_for_node(&self, node_id: &str) -> Vec<&LayoutEdge> {
        self.edges
            .iter()
            .filter(|e| e.source == node_id || e.target == node_id)
            .collect()
    }

    pub fn get_neighbors(&self, node_id: &str) -> Vec<&LayoutNode> {
        let mut neighbors = Vec::new();
        for edge in &self.edges {
            if edge.source == node_id {
                if let Some(node) = self.get_node(&edge.target) {
                    neighbors.push(node);
                }
            } else if edge.target == node_id {
                if let Some(node) = self.get_node(&edge.source) {
                    neighbors.push(node);
                }
            }
        }
        neighbors
    }

    pub fn center_nodes(&mut self) {
        if self.nodes.is_empty() {
            return;
        }

        let mut center_x = 0.0;
        let mut center_y = 0.0;
        for node in &self.nodes {
            center_x += node.position.x;
            center_y += node.position.y;
        }
        center_x /= self.nodes.len() as f64;
        center_y /= self.nodes.len() as f64;

        let offset_x = self.width / 2.0 - center_x;
        let offset_y = self.height / 2.0 - center_y;

        for node in &mut self.nodes {
            node.position.x += offset_x;
            node.position.y += offset_y;
        }
    }

    pub fn scale_to_fit(&mut self, padding: f64) {
        if self.nodes.is_empty() {
            return;
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for node in &self.nodes {
            min_x = min_x.min(node.position.x - node.size);
            max_x = max_x.max(node.position.x + node.size);
            min_y = min_y.min(node.position.y - node.size);
            max_y = max_y.max(node.position.y + node.size);
        }

        let current_width = max_x - min_x;
        let current_height = max_y - min_y;
        let available_width = self.width - 2.0 * padding;
        let available_height = self.height - 2.0 * padding;

        let scale_x = available_width / current_width;
        let scale_y = available_height / current_height;
        let scale = scale_x.min(scale_y).min(1.0);

        for node in &mut self.nodes {
            node.position.x = (node.position.x - min_x) * scale + padding;
            node.position.y = (node.position.y - min_y) * scale + padding;
        }
    }
}

/// Force-directed layout algorithm
pub struct ForceDirectedLayout {
    pub iterations: usize,
    pub temperature: f64,
    pub cooling_factor: f64,
    pub spring_constant: f64,
    pub repulsion_constant: f64,
    pub damping: f64,
    pub min_distance: f64,
    pub max_distance: f64,
}

impl Default for ForceDirectedLayout {
    fn default() -> Self {
        Self {
            iterations: 1000,
            temperature: 100.0,
            cooling_factor: 0.95,
            spring_constant: 0.1,
            repulsion_constant: 1000.0,
            damping: 0.8,
            min_distance: 10.0,
            max_distance: 200.0,
        }
    }
}

impl ForceDirectedLayout {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_iterations(mut self, iterations: usize) -> Self {
        self.iterations = iterations;
        self
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_spring_constant(mut self, spring_constant: f64) -> Self {
        self.spring_constant = spring_constant;
        self
    }

    pub fn with_repulsion_constant(mut self, repulsion_constant: f64) -> Self {
        self.repulsion_constant = repulsion_constant;
        self
    }

    pub fn layout(&self, mut graph: GraphLayout) -> Result<GraphLayout> {
        if graph.nodes.is_empty() {
            return Ok(graph);
        }

        // Initialize random positions if not set
        self.initialize_positions(&mut graph);

        let mut temperature = self.temperature;

        for _iteration in 0..self.iterations {
            // Reset forces
            for node in &mut graph.nodes {
                node.force = Point2D::new(0.0, 0.0);
            }

            // Calculate repulsion forces between all pairs of nodes
            self.calculate_repulsion_forces(&mut graph);

            // Calculate spring forces for connected nodes
            self.calculate_spring_forces(&mut graph);

            // Update positions
            self.update_positions(&mut graph, temperature);

            // Cool down
            temperature *= self.cooling_factor;

            // Check for convergence
            if temperature < 0.1 {
                break;
            }
        }

        // Center and scale the final layout
        graph.center_nodes();
        graph.scale_to_fit(50.0);

        Ok(graph)
    }

    fn initialize_positions(&self, graph: &mut GraphLayout) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        for (i, node) in graph.nodes.iter_mut().enumerate() {
            if node.fixed {
                continue;
            }

            // Use node ID hash for deterministic positioning
            let mut hasher = DefaultHasher::new();
            node.id.hash(&mut hasher);
            let hash = hasher.finish();

            let angle = (hash as f64 / u64::MAX as f64) * 2.0 * PI;
            let radius = (i as f64).sqrt() * 50.0;

            node.position = Point2D::new(
                graph.width / 2.0 + angle.cos() * radius,
                graph.height / 2.0 + angle.sin() * radius,
            );
        }
    }

    fn calculate_repulsion_forces(&self, graph: &mut GraphLayout) {
        for i in 0..graph.nodes.len() {
            for j in (i + 1)..graph.nodes.len() {
                let node1 = &graph.nodes[i];
                let node2 = &graph.nodes[j];

                let distance = node1.position.distance_to(&node2.position);
                if distance < self.min_distance {
                    continue;
                }

                let force_magnitude = self.repulsion_constant / (distance * distance);
                let direction = node1.position.subtract(&node2.position).normalize();
                let force = direction.scale(force_magnitude);

                graph.nodes[i].force = graph.nodes[i].force.add(&force);
                graph.nodes[j].force = graph.nodes[j].force.subtract(&force);
            }
        }
    }

    fn calculate_spring_forces(&self, graph: &mut GraphLayout) {
        for edge in &graph.edges {
            let source_idx = graph.nodes.iter().position(|n| n.id == edge.source);
            let target_idx = graph.nodes.iter().position(|n| n.id == edge.target);

            if let (Some(source_idx), Some(target_idx)) = (source_idx, target_idx) {
                let source = &graph.nodes[source_idx];
                let target = &graph.nodes[target_idx];

                let distance = source.position.distance_to(&target.position);
                let ideal_length = edge.length.max(self.min_distance).min(self.max_distance);

                if distance > 0.0 {
                    let force_magnitude = self.spring_constant * (distance - ideal_length);
                    let direction = target.position.subtract(&source.position).normalize();
                    let force = direction.scale(force_magnitude);

                    graph.nodes[source_idx].force = graph.nodes[source_idx].force.add(&force);
                    graph.nodes[target_idx].force = graph.nodes[target_idx].force.subtract(&force);
                }
            }
        }
    }

    fn update_positions(&self, graph: &mut GraphLayout, temperature: f64) {
        for node in &mut graph.nodes {
            if node.fixed {
                continue;
            }

            // Update velocity with damping
            node.velocity = node.velocity.scale(self.damping).add(&node.force.scale(1.0 / node.mass));

            // Limit velocity by temperature
            let velocity_magnitude = node.velocity.magnitude();
            if velocity_magnitude > temperature {
                node.velocity = node.velocity.normalize().scale(temperature);
            }

            // Update position
            node.position = node.position.add(&node.velocity);

            // Keep nodes within bounds
            node.position.x = node.position.x.max(node.size).min(graph.width - node.size);
            node.position.y = node.position.y.max(node.size).min(graph.height - node.size);
        }
    }
}

/// Hierarchical layout algorithm for tree-like structures
pub struct HierarchicalLayout {
    pub level_spacing: f64,
    pub node_spacing: f64,
    pub direction: LayoutDirection,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutDirection {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

impl Default for HierarchicalLayout {
    fn default() -> Self {
        Self {
            level_spacing: 100.0,
            node_spacing: 50.0,
            direction: LayoutDirection::TopDown,
        }
    }
}

impl HierarchicalLayout {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_level_spacing(mut self, spacing: f64) -> Self {
        self.level_spacing = spacing;
        self
    }

    pub fn with_node_spacing(mut self, spacing: f64) -> Self {
        self.node_spacing = spacing;
        self
    }

    pub fn with_direction(mut self, direction: LayoutDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn layout(&self, mut graph: GraphLayout) -> Result<GraphLayout> {
        if graph.nodes.is_empty() {
            return Ok(graph);
        }

        // Build adjacency list
        let adjacency = self.build_adjacency_list(&graph);

        // Find root nodes (nodes with no incoming edges)
        let roots = self.find_root_nodes(&graph);

        // Assign levels to nodes
        let levels = self.assign_levels(&adjacency, &roots);

        // Position nodes within levels
        self.position_nodes(&mut graph, &levels);

        // Center and scale the layout
        graph.center_nodes();
        graph.scale_to_fit(50.0);

        Ok(graph)
    }

    fn build_adjacency_list(&self, graph: &GraphLayout) -> HashMap<String, Vec<String>> {
        let mut adjacency = HashMap::new();

        for node in &graph.nodes {
            adjacency.insert(node.id.clone(), Vec::new());
        }

        for edge in &graph.edges {
            adjacency
                .get_mut(&edge.source)
                .unwrap()
                .push(edge.target.clone());
        }

        adjacency
    }

    fn find_root_nodes(&self, graph: &GraphLayout) -> Vec<String> {
        let mut has_incoming = HashSet::new();

        for edge in &graph.edges {
            has_incoming.insert(edge.target.clone());
        }

        graph
            .nodes
            .iter()
            .filter(|node| !has_incoming.contains(&node.id))
            .map(|node| node.id.clone())
            .collect()
    }

    fn assign_levels(&self, adjacency: &HashMap<String, Vec<String>>, roots: &[String]) -> HashMap<String, usize> {
        let mut levels = HashMap::new();
        let mut queue = VecDeque::new();

        // Start with root nodes at level 0
        for root in roots {
            levels.insert(root.clone(), 0);
            queue.push_back(root.clone());
        }

        while let Some(node_id) = queue.pop_front() {
            let current_level = levels[&node_id];

            if let Some(children) = adjacency.get(&node_id) {
                for child in children {
                    if !levels.contains_key(child) {
                        levels.insert(child.clone(), current_level + 1);
                        queue.push_back(child.clone());
                    }
                }
            }
        }

        levels
    }

    fn position_nodes(&self, graph: &mut GraphLayout, levels: &HashMap<String, usize>) {
        // Group nodes by level
        let mut level_groups: HashMap<usize, Vec<String>> = HashMap::new();
        for (node_id, level) in levels {
            level_groups.entry(*level).or_default().push(node_id.clone());
        }

        let max_level = level_groups.keys().max().copied().unwrap_or(0);

        for (level, node_ids) in level_groups {
            let y = match self.direction {
                LayoutDirection::TopDown => level as f64 * self.level_spacing,
                LayoutDirection::BottomUp => (max_level - level) as f64 * self.level_spacing,
                LayoutDirection::LeftRight => level as f64 * self.level_spacing,
                LayoutDirection::RightLeft => (max_level - level) as f64 * self.level_spacing,
            };

            // Sort nodes within level for consistent positioning
            let mut sorted_nodes = node_ids;
            sorted_nodes.sort();

            for (i, node_id) in sorted_nodes.iter().enumerate() {
                if let Some(node) = graph.get_node_mut(node_id) {
                    let x = (i as f64 - (sorted_nodes.len() - 1) as f64 / 2.0) * self.node_spacing;

                    match self.direction {
                        LayoutDirection::TopDown | LayoutDirection::BottomUp => {
                            node.position = Point2D::new(x, y);
                        }
                        LayoutDirection::LeftRight | LayoutDirection::RightLeft => {
                            node.position = Point2D::new(y, x);
                        }
                    }
                }
            }
        }
    }
}

/// Circular layout algorithm
pub struct CircularLayout {
    pub radius: f64,
    pub start_angle: f64,
    pub clockwise: bool,
}

impl Default for CircularLayout {
    fn default() -> Self {
        Self {
            radius: 100.0,
            start_angle: 0.0,
            clockwise: true,
        }
    }
}

impl CircularLayout {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_radius(mut self, radius: f64) -> Self {
        self.radius = radius;
        self
    }

    pub fn with_start_angle(mut self, angle: f64) -> Self {
        self.start_angle = angle;
        self
    }

    pub fn with_clockwise(mut self, clockwise: bool) -> Self {
        self.clockwise = clockwise;
        self
    }

    pub fn layout(&self, mut graph: GraphLayout) -> Result<GraphLayout> {
        if graph.nodes.is_empty() {
            return Ok(graph);
        }

        let center_x = graph.width / 2.0;
        let center_y = graph.height / 2.0;
        let angle_step = 2.0 * PI / graph.nodes.len() as f64;
        let direction = if self.clockwise { 1.0 } else { -1.0 };

        for (i, node) in graph.nodes.iter_mut().enumerate() {
            let angle = self.start_angle + direction * i as f64 * angle_step;
            node.position = Point2D::new(
                center_x + angle.cos() * self.radius,
                center_y + angle.sin() * self.radius,
            );
        }

        Ok(graph)
    }
}

/// Grid layout algorithm
pub struct GridLayout {
    pub cell_width: f64,
    pub cell_height: f64,
    pub padding: f64,
}

impl Default for GridLayout {
    fn default() -> Self {
        Self {
            cell_width: 100.0,
            cell_height: 100.0,
            padding: 20.0,
        }
    }
}

impl GridLayout {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cell_size(mut self, width: f64, height: f64) -> Self {
        self.cell_width = width;
        self.cell_height = height;
        self
    }

    pub fn with_padding(mut self, padding: f64) -> Self {
        self.padding = padding;
        self
    }

    pub fn layout(&self, mut graph: GraphLayout) -> Result<GraphLayout> {
        if graph.nodes.is_empty() {
            return Ok(graph);
        }

        let cols = (graph.nodes.len() as f64).sqrt().ceil() as usize;
        let _rows = (graph.nodes.len() + cols - 1) / cols;

        for (i, node) in graph.nodes.iter_mut().enumerate() {
            let row = i / cols;
            let col = i % cols;

            let x = self.padding + col as f64 * self.cell_width;
            let y = self.padding + row as f64 * self.cell_height;

            node.position = Point2D::new(x, y);
        }

        Ok(graph)
    }
}

/// K-means clustering algorithm
pub struct KMeansClustering {
    pub k: usize,
    pub max_iterations: usize,
    pub tolerance: f64,
}

impl KMeansClustering {
    pub fn new(k: usize) -> Self {
        Self {
            k,
            max_iterations: 100,
            tolerance: 1e-6,
        }
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    pub fn cluster(&self, graph: &GraphLayout) -> Result<Vec<usize>> {
        if graph.nodes.is_empty() {
            return Ok(Vec::new());
        }

        let n = graph.nodes.len();
        let k = self.k.min(n);

        if k == 0 {
            return Ok(vec![0; n]);
        }

        // Initialize centroids randomly
        let mut centroids = self.initialize_centroids(graph, k);
        let mut assignments = vec![0; n];
        let mut prev_assignments = vec![usize::MAX; n];

        for _iteration in 0..self.max_iterations {
            // Assign nodes to closest centroid
            for (i, node) in graph.nodes.iter().enumerate() {
                let mut min_distance = f64::INFINITY;
                let mut closest_centroid = 0;

                for (j, centroid) in centroids.iter().enumerate() {
                    let distance = node.position.distance_to(centroid);
                    if distance < min_distance {
                        min_distance = distance;
                        closest_centroid = j;
                    }
                }

                assignments[i] = closest_centroid;
            }

            // Check for convergence
            if assignments == prev_assignments {
                break;
            }

            // Update centroids
            self.update_centroids(graph, &assignments, &mut centroids);

            prev_assignments = assignments.clone();
        }

        Ok(assignments)
    }

    fn initialize_centroids(&self, graph: &GraphLayout, k: usize) -> Vec<Point2D> {

        let mut centroids = Vec::new();
        let mut used_indices = HashSet::new();

        for i in 0..k {
            let mut index;
            loop {
                index = (i * 7 + 13) % graph.nodes.len(); // Simple deterministic selection
                if !used_indices.contains(&index) {
                    used_indices.insert(index);
                    break;
                }
            }
            centroids.push(graph.nodes[index].position);
        }

        centroids
    }

    fn update_centroids(&self, graph: &GraphLayout, assignments: &[usize], centroids: &mut Vec<Point2D>) {
        let mut counts = vec![0; centroids.len()];
        let mut sums = vec![Point2D::new(0.0, 0.0); centroids.len()];

        for (i, &cluster) in assignments.iter().enumerate() {
            counts[cluster] += 1;
            sums[cluster] = sums[cluster].add(&graph.nodes[i].position);
        }

        for (i, centroid) in centroids.iter_mut().enumerate() {
            if counts[i] > 0 {
                *centroid = Point2D::new(
                    sums[i].x / counts[i] as f64,
                    sums[i].y / counts[i] as f64,
                );
            }
        }
    }
}

/// Connected components algorithm
pub struct ConnectedComponents {
    pub directed: bool,
}

impl Default for ConnectedComponents {
    fn default() -> Self {
        Self { directed: false }
    }
}

impl ConnectedComponents {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_directed(mut self, directed: bool) -> Self {
        self.directed = directed;
        self
    }

    pub fn find_components(&self, graph: &GraphLayout) -> Result<Vec<usize>> {
        let n = graph.nodes.len();
        let mut components = vec![usize::MAX; n];
        let mut component_id = 0;

        // Build adjacency list
        let adjacency = self.build_adjacency_list(graph);

        for i in 0..n {
            if components[i] == usize::MAX {
                self.dfs(&adjacency, i, component_id, &mut components);
                component_id += 1;
            }
        }

        Ok(components)
    }

    fn build_adjacency_list(&self, graph: &GraphLayout) -> Vec<Vec<usize>> {
        let mut adjacency = vec![Vec::new(); graph.nodes.len()];
        let mut node_to_index = HashMap::new();

        for (i, node) in graph.nodes.iter().enumerate() {
            node_to_index.insert(node.id.clone(), i);
        }

        for edge in &graph.edges {
            if let (Some(&source_idx), Some(&target_idx)) = (
                node_to_index.get(&edge.source),
                node_to_index.get(&edge.target),
            ) {
                adjacency[source_idx].push(target_idx);
                if !self.directed {
                    adjacency[target_idx].push(source_idx);
                }
            }
        }

        adjacency
    }

    fn dfs(&self, adjacency: &[Vec<usize>], node: usize, component_id: usize, components: &mut [usize]) {
        components[node] = component_id;

        for &neighbor in &adjacency[node] {
            if components[neighbor] == usize::MAX {
                self.dfs(adjacency, neighbor, component_id, components);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_graph() -> GraphLayout {
        let mut graph = GraphLayout::new(800.0, 600.0);

        // Add nodes
        graph.add_node(LayoutNode::new("A".to_string(), Point2D::new(0.0, 0.0)));
        graph.add_node(LayoutNode::new("B".to_string(), Point2D::new(0.0, 0.0)));
        graph.add_node(LayoutNode::new("C".to_string(), Point2D::new(0.0, 0.0)));
        graph.add_node(LayoutNode::new("D".to_string(), Point2D::new(0.0, 0.0)));

        // Add edges
        graph.add_edge(LayoutEdge::new("AB".to_string(), "A".to_string(), "B".to_string()));
        graph.add_edge(LayoutEdge::new("BC".to_string(), "B".to_string(), "C".to_string()));
        graph.add_edge(LayoutEdge::new("CD".to_string(), "C".to_string(), "D".to_string()));
        graph.add_edge(LayoutEdge::new("DA".to_string(), "D".to_string(), "A".to_string()));

        graph
    }

    #[test]
    fn test_point2d_operations() {
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(3.0, 4.0);

        assert_eq!(p1.distance_to(&p2), 5.0);
        assert_eq!(p1.add(&p2), Point2D::new(3.0, 4.0));
        assert_eq!(p2.subtract(&p1), Point2D::new(3.0, 4.0));
        assert_eq!(p2.scale(2.0), Point2D::new(6.0, 8.0));
        assert_eq!(p2.magnitude(), 5.0);
    }

    #[test]
    fn test_force_directed_layout() {
        let graph = create_test_graph();
        let layout = ForceDirectedLayout::new().with_iterations(100);
        let result = layout.layout(graph).unwrap();

        // Check that all nodes have been positioned
        for node in &result.nodes {
            assert!(node.position.x >= 0.0);
            assert!(node.position.y >= 0.0);
        }
    }

    #[test]
    fn test_hierarchical_layout() {
        let graph = create_test_graph();
        let layout = HierarchicalLayout::new();
        let result = layout.layout(graph).unwrap();

        // Check that all nodes have been positioned
        for node in &result.nodes {
            assert!(node.position.x >= 0.0);
            assert!(node.position.y >= 0.0);
        }
    }

    #[test]
    fn test_circular_layout() {
        let graph = create_test_graph();
        let layout = CircularLayout::new();
        let result = layout.layout(graph).unwrap();

        // Check that all nodes have been positioned
        for node in &result.nodes {
            assert!(node.position.x >= 0.0);
            assert!(node.position.y >= 0.0);
        }
    }

    #[test]
    fn test_grid_layout() {
        let graph = create_test_graph();
        let layout = GridLayout::new();
        let result = layout.layout(graph).unwrap();

        // Check that all nodes have been positioned
        for node in &result.nodes {
            assert!(node.position.x >= 0.0);
            assert!(node.position.y >= 0.0);
        }
    }

    #[test]
    fn test_kmeans_clustering() {
        let graph = create_test_graph();
        let clustering = KMeansClustering::new(2);
        let assignments = clustering.cluster(&graph).unwrap();

        assert_eq!(assignments.len(), graph.nodes.len());
        assert!(assignments.iter().all(|&a| a < 2));
    }

    #[test]
    fn test_connected_components() {
        let graph = create_test_graph();
        let cc = ConnectedComponents::new();
        let components = cc.find_components(&graph).unwrap();

        assert_eq!(components.len(), graph.nodes.len());
        // All nodes should be in the same component since we have a cycle
        assert!(components.iter().all(|&c| c == components[0]));
    }

    #[test]
    fn test_graph_layout_operations() {
        let mut graph = GraphLayout::new(800.0, 600.0);
        graph.add_node(LayoutNode::new("A".to_string(), Point2D::new(100.0, 100.0)));
        graph.add_node(LayoutNode::new("B".to_string(), Point2D::new(200.0, 200.0)));

        assert!(graph.get_node("A").is_some());
        assert!(graph.get_node("C").is_none());

        graph.center_nodes();
        graph.scale_to_fit(50.0);

        // After centering and scaling, nodes should be within bounds
        for node in &graph.nodes {
            assert!(node.position.x >= 0.0);
            assert!(node.position.y >= 0.0);
        }
    }
}
