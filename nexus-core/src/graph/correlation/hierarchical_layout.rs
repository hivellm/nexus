//! Hierarchical Call Graph Layout
//!
//! This module provides specialized hierarchical layout algorithms for call graphs,
//! optimizing the visualization of function call hierarchies and dependencies.

use crate::Result;
use crate::graph::construction::{LayoutDirection, Point2D};
use crate::graph::correlation::{CorrelationGraph, EdgeType, NodeType};
use std::collections::{HashMap, HashSet, VecDeque};

/// Hierarchical call graph layout configuration
#[derive(Debug, Clone)]
pub struct HierarchicalCallGraphConfig {
    /// Spacing between hierarchy levels
    pub level_spacing: f64,
    /// Spacing between nodes within the same level
    pub node_spacing: f64,
    /// Layout direction
    pub direction: LayoutDirection,
    /// Whether to group functions by their containing modules
    pub group_by_module: bool,
    /// Whether to show call flow arrows
    pub show_call_flow: bool,
    /// Minimum distance between nodes to prevent overlap
    pub min_node_distance: f64,
    /// Whether to use curved edges for better readability
    pub use_curved_edges: bool,
    /// Padding around the entire layout
    pub padding: f64,
}

impl Default for HierarchicalCallGraphConfig {
    fn default() -> Self {
        Self {
            level_spacing: 120.0,
            node_spacing: 80.0,
            direction: LayoutDirection::TopDown,
            group_by_module: true,
            show_call_flow: true,
            min_node_distance: 30.0,
            use_curved_edges: true,
            padding: 50.0,
        }
    }
}

/// Hierarchical call graph layout engine
pub struct HierarchicalCallGraphLayout {
    config: HierarchicalCallGraphConfig,
}

impl HierarchicalCallGraphLayout {
    /// Create a new hierarchical call graph layout engine
    pub fn new(config: HierarchicalCallGraphConfig) -> Self {
        Self { config }
    }

    /// Create a new layout engine with default configuration
    pub fn with_default_config() -> Self {
        Self::new(HierarchicalCallGraphConfig::default())
    }

    /// Apply hierarchical layout to a call graph
    pub fn layout(&self, mut graph: CorrelationGraph) -> Result<CorrelationGraph> {
        if graph.nodes.is_empty() {
            return Ok(graph);
        }

        // Convert to internal layout format
        let mut layout_graph = self.convert_to_layout_graph(&graph)?;

        // Build call hierarchy
        let hierarchy = self.build_call_hierarchy(&layout_graph)?;

        // Assign hierarchy levels
        let levels = self.assign_hierarchy_levels(&hierarchy)?;

        // Position nodes based on hierarchy
        self.position_nodes(&mut layout_graph, &levels)?;

        // Convert back to CorrelationGraph format
        self.convert_from_layout_graph(layout_graph, &mut graph)?;

        Ok(graph)
    }

    /// Convert CorrelationGraph to internal layout format
    fn convert_to_layout_graph(&self, graph: &CorrelationGraph) -> Result<LayoutGraph> {
        let mut layout_graph = LayoutGraph::new();

        // Add nodes
        for node in &graph.nodes {
            let layout_node = LayoutNode {
                id: node.id.clone(),
                node_type: node.node_type,
                label: node.label.clone(),
                module: self.extract_module_from_id(&node.id),
                metadata: node
                    .metadata
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_string()))
                    .collect(),
                position: node
                    .position
                    .map(|(x, y)| Point2D::new(x as f64, y as f64))
                    .unwrap_or(Point2D::new(0.0, 0.0)),
                size: node.size.unwrap_or(1.0) as f64,
                level: 0,
                is_root: false,
                is_leaf: false,
            };
            layout_graph.add_node(layout_node);
        }

        // Add edges
        for edge in &graph.edges {
            let layout_edge = LayoutEdge {
                id: edge.id.clone(),
                source: edge.source.clone(),
                target: edge.target.clone(),
                edge_type: edge.edge_type,
                weight: edge.weight as f64,
                metadata: edge
                    .metadata
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_string()))
                    .collect(),
                label: edge.label.clone(),
            };
            layout_graph.add_edge(layout_edge);
        }

        Ok(layout_graph)
    }

    /// Build call hierarchy from the graph
    fn build_call_hierarchy(&self, graph: &LayoutGraph) -> Result<CallHierarchy> {
        let mut hierarchy = CallHierarchy::new();

        // Build adjacency lists
        let mut outgoing_calls = HashMap::new();
        let mut incoming_calls = HashMap::new();

        for node in &graph.nodes {
            outgoing_calls.insert(node.id.clone(), Vec::new());
            incoming_calls.insert(node.id.clone(), Vec::new());
        }

        for edge in &graph.edges {
            if edge.edge_type == EdgeType::Calls || edge.edge_type == EdgeType::Uses {
                outgoing_calls
                    .get_mut(&edge.source)
                    .unwrap()
                    .push(edge.target.clone());
                incoming_calls
                    .get_mut(&edge.target)
                    .unwrap()
                    .push(edge.source.clone());
            }
        }

        // Find root nodes (functions with no incoming calls)
        let mut roots = Vec::new();
        for node in &graph.nodes {
            if node.node_type == NodeType::Function
                && incoming_calls
                    .get(&node.id)
                    .is_none_or(|calls| calls.is_empty())
            {
                roots.push(node.id.clone());
            }
        }

        // If no function roots found, use module roots
        if roots.is_empty() {
            for node in &graph.nodes {
                if node.node_type == NodeType::Module
                    && incoming_calls
                        .get(&node.id)
                        .is_none_or(|calls| calls.is_empty())
                {
                    roots.push(node.id.clone());
                }
            }
        }

        // Build hierarchy using BFS
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        for root in roots {
            queue.push_back((root.clone(), 0));
            visited.insert(root.clone());
            hierarchy.add_node(root, 0, true, false);
        }

        while let Some((node_id, level)) = queue.pop_front() {
            if let Some(children) = outgoing_calls.get(&node_id) {
                for child in children {
                    if !visited.contains(child) {
                        visited.insert(child.clone());
                        let is_leaf = outgoing_calls
                            .get(child)
                            .is_none_or(|calls| calls.is_empty());
                        hierarchy.add_node(child.clone(), level + 1, false, is_leaf);
                        hierarchy.add_relationship(node_id.clone(), child.clone());
                        queue.push_back((child.clone(), level + 1));
                    }
                }
            }
        }

        // Add orphaned nodes
        for node in &graph.nodes {
            if !visited.contains(&node.id) {
                hierarchy.add_node(node.id.clone(), 0, false, true);
            }
        }

        Ok(hierarchy)
    }

    /// Assign hierarchy levels to nodes
    fn assign_hierarchy_levels(&self, hierarchy: &CallHierarchy) -> Result<HashMap<String, usize>> {
        let mut levels = HashMap::new();

        // Start with root nodes at level 0
        let mut queue = VecDeque::new();
        for (node_id, node_info) in &hierarchy.nodes {
            if node_info.is_root {
                levels.insert(node_id.clone(), 0);
                queue.push_back(node_id.clone());
            }
        }

        // BFS to assign levels
        while let Some(node_id) = queue.pop_front() {
            let current_level = levels[&node_id];

            if let Some(children) = hierarchy.relationships.get(&node_id) {
                for child in children {
                    if !levels.contains_key(child) {
                        levels.insert(child.clone(), current_level + 1);
                        queue.push_back(child.clone());
                    }
                }
            }
        }

        // Assign levels to orphaned nodes
        for node_id in hierarchy.nodes.keys() {
            if !levels.contains_key(node_id) {
                levels.insert(node_id.clone(), 0);
            }
        }

        Ok(levels)
    }

    /// Position nodes based on hierarchy levels
    fn position_nodes(
        &self,
        graph: &mut LayoutGraph,
        levels: &HashMap<String, usize>,
    ) -> Result<()> {
        // Group nodes by level
        let mut level_groups: HashMap<usize, Vec<String>> = HashMap::new();
        for (node_id, level) in levels {
            level_groups
                .entry(*level)
                .or_default()
                .push(node_id.clone());
        }

        let max_level = level_groups.keys().max().copied().unwrap_or(0);

        // Position nodes level by level
        for (level, node_ids) in level_groups {
            let level_y = self.calculate_level_position(level, max_level);

            // Sort nodes within level for consistent positioning
            let mut sorted_nodes = node_ids;
            sorted_nodes.sort();

            // Group by module if configured
            if self.config.group_by_module {
                self.position_nodes_grouped(graph, &sorted_nodes, level_y)?;
            } else {
                self.position_nodes_linear(graph, &sorted_nodes, level_y)?;
            }
        }

        Ok(())
    }

    /// Calculate Y position for a hierarchy level
    fn calculate_level_position(&self, level: usize, max_level: usize) -> f64 {
        match self.config.direction {
            LayoutDirection::TopDown => {
                level as f64 * self.config.level_spacing + self.config.padding
            }
            LayoutDirection::BottomUp => {
                (max_level - level) as f64 * self.config.level_spacing + self.config.padding
            }
            LayoutDirection::LeftRight => {
                level as f64 * self.config.level_spacing + self.config.padding
            }
            LayoutDirection::RightLeft => {
                (max_level - level) as f64 * self.config.level_spacing + self.config.padding
            }
        }
    }

    /// Position nodes in a linear arrangement
    fn position_nodes_linear(
        &self,
        graph: &mut LayoutGraph,
        node_ids: &[String],
        level_y: f64,
    ) -> Result<()> {
        for (i, node_id) in node_ids.iter().enumerate() {
            if let Some(node) = graph.get_node_mut(node_id) {
                let x = (i as f64 - (node_ids.len() - 1) as f64 / 2.0) * self.config.node_spacing;

                match self.config.direction {
                    LayoutDirection::TopDown | LayoutDirection::BottomUp => {
                        node.position = Point2D::new(x, level_y);
                    }
                    LayoutDirection::LeftRight | LayoutDirection::RightLeft => {
                        node.position = Point2D::new(level_y, x);
                    }
                }
            }
        }
        Ok(())
    }

    /// Position nodes grouped by module
    fn position_nodes_grouped(
        &self,
        graph: &mut LayoutGraph,
        node_ids: &[String],
        level_y: f64,
    ) -> Result<()> {
        // Group nodes by module
        let mut module_groups: HashMap<String, Vec<String>> = HashMap::new();
        for node_id in node_ids {
            if let Some(node) = graph.get_node(node_id) {
                let module = node.module.clone().unwrap_or_else(|| "unknown".to_string());
                module_groups
                    .entry(module)
                    .or_default()
                    .push(node_id.clone());
            }
        }

        let mut current_x = -(node_ids.len() as f64 * self.config.node_spacing) / 2.0;

        for (_module, module_nodes) in module_groups {
            // Position nodes within module
            for (i, node_id) in module_nodes.iter().enumerate() {
                if let Some(node) = graph.get_node_mut(node_id) {
                    let x = current_x + i as f64 * self.config.node_spacing;

                    match self.config.direction {
                        LayoutDirection::TopDown | LayoutDirection::BottomUp => {
                            node.position = Point2D::new(x, level_y);
                        }
                        LayoutDirection::LeftRight | LayoutDirection::RightLeft => {
                            node.position = Point2D::new(level_y, x);
                        }
                    }
                }
            }

            current_x +=
                module_nodes.len() as f64 * self.config.node_spacing + self.config.node_spacing;
        }

        Ok(())
    }

    /// Convert from internal layout format back to CorrelationGraph
    fn convert_from_layout_graph(
        &self,
        layout_graph: LayoutGraph,
        graph: &mut CorrelationGraph,
    ) -> Result<()> {
        for layout_node in layout_graph.nodes {
            if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == layout_node.id) {
                node.position =
                    Some((layout_node.position.x as f32, layout_node.position.y as f32));
                node.size = Some(layout_node.size as f32);
            }
        }
        Ok(())
    }

    /// Extract module name from node ID
    fn extract_module_from_id(&self, node_id: &str) -> Option<String> {
        if let Some(path) = node_id.strip_prefix("file:") {
            path.split('/').next_back().map(|s| s.to_string())
        } else if let Some(stripped) = node_id.strip_prefix("func:") {
            let parts: Vec<&str> = stripped.split(':').collect();
            if parts.len() >= 2 {
                Some(parts[0].to_string())
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Internal layout graph structure
#[derive(Debug, Clone)]
struct LayoutGraph {
    nodes: Vec<LayoutNode>,
    edges: Vec<LayoutEdge>,
}

impl LayoutGraph {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn add_node(&mut self, node: LayoutNode) {
        self.nodes.push(node);
    }

    fn add_edge(&mut self, edge: LayoutEdge) {
        self.edges.push(edge);
    }

    fn get_node(&self, id: &str) -> Option<&LayoutNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    fn get_node_mut(&mut self, id: &str) -> Option<&mut LayoutNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
}

/// Internal layout node structure
#[derive(Debug, Clone)]
struct LayoutNode {
    id: String,
    node_type: NodeType,
    label: String,
    module: Option<String>,
    metadata: HashMap<String, String>,
    position: Point2D,
    size: f64,
    level: usize,
    is_root: bool,
    is_leaf: bool,
}

/// Internal layout edge structure
#[derive(Debug, Clone)]
struct LayoutEdge {
    id: String,
    source: String,
    target: String,
    edge_type: EdgeType,
    weight: f64,
    metadata: HashMap<String, String>,
    label: Option<String>,
}

/// Call hierarchy structure
#[derive(Debug, Clone)]
struct CallHierarchy {
    nodes: HashMap<String, NodeInfo>,
    relationships: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
struct NodeInfo {
    level: usize,
    is_root: bool,
    is_leaf: bool,
}

impl CallHierarchy {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            relationships: HashMap::new(),
        }
    }

    fn add_node(&mut self, id: String, level: usize, is_root: bool, is_leaf: bool) {
        self.nodes.insert(
            id,
            NodeInfo {
                level,
                is_root,
                is_leaf,
            },
        );
    }

    fn add_relationship(&mut self, parent: String, child: String) {
        self.relationships.entry(parent).or_default().push(child);
    }
}

/// Call graph layout utilities
pub struct CallGraphLayoutUtils;

impl CallGraphLayoutUtils {
    /// Analyze call graph structure and suggest optimal layout parameters
    pub fn analyze_graph_structure(graph: &CorrelationGraph) -> HierarchicalCallGraphConfig {
        let node_count = graph.nodes.len();
        let edge_count = graph.edges.len();

        // Calculate complexity metrics
        let avg_connections = if node_count > 0 {
            edge_count as f64 / node_count as f64
        } else {
            0.0
        };
        let is_dense = avg_connections > 3.0;

        // Adjust spacing based on graph size and complexity
        let level_spacing = if node_count > 50 { 150.0 } else { 120.0 };
        let node_spacing = if is_dense { 60.0 } else { 80.0 };

        HierarchicalCallGraphConfig {
            level_spacing,
            node_spacing,
            direction: LayoutDirection::TopDown,
            group_by_module: node_count > 20,
            show_call_flow: true,
            min_node_distance: if is_dense { 20.0 } else { 30.0 },
            use_curved_edges: is_dense,
            padding: 50.0,
        }
    }

    /// Calculate layout bounds for the graph
    pub fn calculate_layout_bounds(
        graph: &CorrelationGraph,
        config: &HierarchicalCallGraphConfig,
    ) -> (f64, f64) {
        let node_count = graph.nodes.len();
        let max_levels = (node_count as f64).sqrt().ceil() as usize;

        let width = if config.direction == LayoutDirection::TopDown
            || config.direction == LayoutDirection::BottomUp
        {
            node_count as f64 * config.node_spacing + 2.0 * config.padding
        } else {
            max_levels as f64 * config.level_spacing + 2.0 * config.padding
        };

        let height = if config.direction == LayoutDirection::TopDown
            || config.direction == LayoutDirection::BottomUp
        {
            max_levels as f64 * config.level_spacing + 2.0 * config.padding
        } else {
            node_count as f64 * config.node_spacing + 2.0 * config.padding
        };

        (width, height)
    }

    /// Generate layout statistics
    pub fn generate_layout_stats(graph: &CorrelationGraph) -> LayoutStatistics {
        let node_count = graph.nodes.len();
        let edge_count = graph.edges.len();

        let function_count = graph
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function)
            .count();

        let module_count = graph
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Module)
            .count();

        let avg_connections = if node_count > 0 {
            edge_count as f64 / node_count as f64
        } else {
            0.0
        };

        LayoutStatistics {
            total_nodes: node_count,
            total_edges: edge_count,
            function_nodes: function_count,
            module_nodes: module_count,
            average_connections: avg_connections,
            complexity_score: Self::calculate_complexity_score(
                node_count,
                edge_count,
                avg_connections,
            ),
        }
    }

    fn calculate_complexity_score(
        node_count: usize,
        edge_count: usize,
        avg_connections: f64,
    ) -> f64 {
        let size_factor = (node_count as f64).log2().max(1.0);
        let density_factor = avg_connections;
        let connectivity_factor = if node_count > 0 {
            edge_count as f64 / (node_count * (node_count - 1)) as f64
        } else {
            0.0
        };

        (size_factor + density_factor + connectivity_factor * 10.0) / 3.0
    }
}

/// Layout statistics
#[derive(Debug, Clone)]
pub struct LayoutStatistics {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub function_nodes: usize,
    pub module_nodes: usize,
    pub average_connections: f64,
    pub complexity_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::correlation::{GraphEdge, GraphNode, GraphType};

    fn create_test_call_graph() -> CorrelationGraph {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Call Graph".to_string());

        // Add module nodes
        let module1 = GraphNode {
            id: "file:module1.rs".to_string(),
            node_type: NodeType::Module,
            label: "module1.rs".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(module1).unwrap();

        let module2 = GraphNode {
            id: "file:module2.rs".to_string(),
            node_type: NodeType::Module,
            label: "module2.rs".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(module2).unwrap();

        // Add function nodes
        let func1 = GraphNode {
            id: "func:module1.rs:main".to_string(),
            node_type: NodeType::Function,
            label: "main".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(func1).unwrap();

        let func2 = GraphNode {
            id: "func:module1.rs:helper".to_string(),
            node_type: NodeType::Function,
            label: "helper".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(func2).unwrap();

        let func3 = GraphNode {
            id: "func:module2.rs:utility".to_string(),
            node_type: NodeType::Function,
            label: "utility".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(func3).unwrap();

        // Add edges
        let edge1 = GraphEdge {
            id: "edge1".to_string(),
            source: "file:module1.rs".to_string(),
            target: "func:module1.rs:main".to_string(),
            edge_type: EdgeType::Uses,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        };
        graph.add_edge(edge1).unwrap();

        let edge2 = GraphEdge {
            id: "edge2".to_string(),
            source: "func:module1.rs:main".to_string(),
            target: "func:module1.rs:helper".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        };
        graph.add_edge(edge2).unwrap();

        let edge3 = GraphEdge {
            id: "edge3".to_string(),
            source: "func:module1.rs:helper".to_string(),
            target: "func:module2.rs:utility".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        };
        graph.add_edge(edge3).unwrap();

        graph
    }

    #[test]
    fn test_hierarchical_layout_creation() {
        let config = HierarchicalCallGraphConfig::default();
        let layout = HierarchicalCallGraphLayout::new(config);
        assert_eq!(layout.config.level_spacing, 120.0);
    }

    #[test]
    fn test_hierarchical_layout_application() {
        let graph = create_test_call_graph();
        let layout = HierarchicalCallGraphLayout::with_default_config();
        let result = layout.layout(graph).unwrap();

        // Check that all nodes have been positioned
        for node in &result.nodes {
            assert!(node.position.is_some());
        }
    }

    #[test]
    fn test_layout_utils_analysis() {
        let graph = create_test_call_graph();
        let config = CallGraphLayoutUtils::analyze_graph_structure(&graph);

        assert!(config.level_spacing > 0.0);
        assert!(config.node_spacing > 0.0);
    }

    #[test]
    fn test_layout_bounds_calculation() {
        let graph = create_test_call_graph();
        let config = HierarchicalCallGraphConfig::default();
        let (width, height) = CallGraphLayoutUtils::calculate_layout_bounds(&graph, &config);

        assert!(width > 0.0);
        assert!(height > 0.0);
    }

    #[test]
    fn test_layout_statistics() {
        let graph = create_test_call_graph();
        let stats = CallGraphLayoutUtils::generate_layout_stats(&graph);

        assert_eq!(stats.total_nodes, 5);
        assert_eq!(stats.function_nodes, 3);
        assert_eq!(stats.module_nodes, 2);
    }
}
