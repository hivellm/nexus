//! Call Graph Filtering and Search
//!
//! This module provides comprehensive filtering and search capabilities for call graphs,
//! allowing users to find specific nodes, edges, and patterns within the graph structure.

use crate::Result;
use crate::graph::correlation::{CorrelationGraph, EdgeType, GraphEdge, GraphNode, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

/// Filter criteria for call graph nodes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeFilter {
    /// Filter by node types
    pub node_types: Option<Vec<NodeType>>,
    /// Filter by node labels (exact match)
    pub labels: Option<Vec<String>>,
    /// Filter by node labels (contains)
    pub label_contains: Option<Vec<String>>,
    /// Filter by metadata fields
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    /// Filter by module/file path
    pub module_paths: Option<Vec<String>>,
    /// Filter by function names
    pub function_names: Option<Vec<String>>,
    /// Filter by position bounds (x_min, y_min, x_max, y_max)
    pub position_bounds: Option<(f32, f32, f32, f32)>,
    /// Filter by size range (min_size, max_size)
    pub size_range: Option<(f32, f32)>,
    /// Filter by color
    pub colors: Option<Vec<String>>,
    /// Case sensitive matching for text fields
    pub case_sensitive: bool,
}

/// Filter criteria for call graph edges
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EdgeFilter {
    /// Filter by edge types
    pub edge_types: Option<Vec<EdgeType>>,
    /// Filter by source node IDs
    pub source_nodes: Option<Vec<String>>,
    /// Filter by target node IDs
    pub target_nodes: Option<Vec<String>>,
    /// Filter by weight range (min_weight, max_weight)
    pub weight_range: Option<(f32, f32)>,
    /// Filter by edge labels
    pub labels: Option<Vec<String>>,
    /// Filter by edge labels (contains)
    pub label_contains: Option<Vec<String>>,
    /// Filter by metadata fields
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    /// Filter by recursive calls only
    pub recursive_only: Option<bool>,
    /// Case sensitive matching for text fields
    pub case_sensitive: bool,
}

/// Search criteria for call graphs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraphSearch {
    /// Text search query
    pub query: String,
    /// Search in node labels
    pub search_labels: bool,
    /// Search in node metadata
    pub search_metadata: bool,
    /// Search in edge labels
    pub search_edge_labels: bool,
    /// Search in edge metadata
    pub search_edge_metadata: bool,
    /// Case sensitive search
    pub case_sensitive: bool,
    /// Use regex patterns
    pub use_regex: bool,
    /// Search in function names only
    pub function_names_only: bool,
    /// Search in module paths only
    pub module_paths_only: bool,
}

impl Default for CallGraphSearch {
    fn default() -> Self {
        Self {
            query: String::new(),
            search_labels: true,
            search_metadata: true,
            search_edge_labels: true,
            search_edge_metadata: true,
            case_sensitive: false,
            use_regex: false,
            function_names_only: false,
            module_paths_only: false,
        }
    }
}

/// Path-based search criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathSearch {
    /// Starting node ID
    pub start_node: Option<String>,
    /// Ending node ID
    pub end_node: Option<String>,
    /// Maximum path length
    pub max_length: Option<usize>,
    /// Minimum path length
    pub min_length: Option<usize>,
    /// Allowed edge types in path
    pub allowed_edge_types: Option<Vec<EdgeType>>,
    /// Forbidden edge types in path
    pub forbidden_edge_types: Option<Vec<EdgeType>>,
    /// Include cycles in path search
    pub include_cycles: bool,
    /// Find all paths or just shortest
    pub find_all_paths: bool,
}

impl Default for PathSearch {
    fn default() -> Self {
        Self {
            start_node: None,
            end_node: None,
            max_length: Some(10),
            min_length: Some(1),
            allowed_edge_types: None,
            forbidden_edge_types: None,
            include_cycles: false,
            find_all_paths: false,
        }
    }
}

/// Call graph filtering and search engine
pub struct CallGraphFilter {
    graph: CorrelationGraph,
}

impl CallGraphFilter {
    /// Create a new call graph filter
    pub fn new(graph: CorrelationGraph) -> Self {
        Self { graph }
    }

    /// Filter nodes based on criteria
    pub fn filter_nodes(&self, filter: &NodeFilter) -> Result<Vec<&GraphNode>> {
        let mut filtered_nodes = Vec::new();

        for node in &self.graph.nodes {
            if self.matches_node_filter(node, filter)? {
                filtered_nodes.push(node);
            }
        }

        Ok(filtered_nodes)
    }

    /// Filter edges based on criteria
    pub fn filter_edges(&self, filter: &EdgeFilter) -> Result<Vec<&GraphEdge>> {
        let mut filtered_edges = Vec::new();

        for edge in &self.graph.edges {
            if self.matches_edge_filter(edge, filter)? {
                filtered_edges.push(edge);
            }
        }

        Ok(filtered_edges)
    }

    /// Search for nodes and edges matching the search criteria
    pub fn search(&self, search: &CallGraphSearch) -> Result<CallGraphSearchResult> {
        let mut matching_nodes = Vec::new();
        let mut matching_edges = Vec::new();

        // Search nodes
        for node in &self.graph.nodes {
            if self.matches_search_criteria(node, search)? {
                matching_nodes.push(node.clone());
            }
        }

        // Search edges
        for edge in &self.graph.edges {
            if self.matches_edge_search_criteria(edge, search)? {
                matching_edges.push(edge.clone());
            }
        }

        let total_matches = matching_nodes.len() + matching_edges.len();
        Ok(CallGraphSearchResult {
            matching_nodes,
            matching_edges,
            total_matches,
        })
    }

    /// Find paths between nodes
    pub fn find_paths(&self, path_search: &PathSearch) -> Result<Vec<CallGraphPath>> {
        let mut paths = Vec::new();

        if let Some(start) = &path_search.start_node {
            if let Some(end) = &path_search.end_node {
                // Find specific path between two nodes
                let found_paths = self.find_paths_between(start, end, path_search)?;
                paths.extend(found_paths);
            } else {
                // Find all paths from start node
                let found_paths = self.find_paths_from(start, path_search)?;
                paths.extend(found_paths);
            }
        } else if let Some(end) = &path_search.end_node {
            // Find all paths to end node
            let found_paths = self.find_paths_to(end, path_search)?;
            paths.extend(found_paths);
        } else {
            // Find all possible paths
            let found_paths = self.find_all_paths(path_search)?;
            paths.extend(found_paths);
        }

        Ok(paths)
    }

    /// Get nodes by type
    pub fn get_nodes_by_type(&self, node_type: NodeType) -> Vec<&GraphNode> {
        self.graph
            .nodes
            .iter()
            .filter(|node| node.node_type == node_type)
            .collect()
    }

    /// Get edges by type
    pub fn get_edges_by_type(&self, edge_type: EdgeType) -> Vec<&GraphEdge> {
        self.graph
            .edges
            .iter()
            .filter(|edge| edge.edge_type == edge_type)
            .collect()
    }

    /// Get nodes connected to a specific node
    pub fn get_connected_nodes(&self, node_id: &str) -> Result<Vec<&GraphNode>> {
        let mut connected_nodes = Vec::new();
        let mut node_ids = HashSet::new();

        // Find all edges connected to this node
        for edge in &self.graph.edges {
            if edge.source == node_id {
                node_ids.insert(&edge.target);
            } else if edge.target == node_id {
                node_ids.insert(&edge.source);
            }
        }

        // Get the actual nodes
        for node in &self.graph.nodes {
            if node_ids.contains(&node.id) {
                connected_nodes.push(node);
            }
        }

        Ok(connected_nodes)
    }

    /// Get edges connected to a specific node
    pub fn get_connected_edges(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.graph
            .edges
            .iter()
            .filter(|edge| edge.source == node_id || edge.target == node_id)
            .collect()
    }

    /// Get nodes by module path
    pub fn get_nodes_by_module(&self, module_path: &str) -> Vec<&GraphNode> {
        self.graph
            .nodes
            .iter()
            .filter(|node| {
                node.id.starts_with(&format!("file:{}", module_path))
                    || node.id.contains(&format!(":{}:", module_path))
            })
            .collect()
    }

    /// Get function nodes by name
    pub fn get_functions_by_name(&self, function_name: &str) -> Vec<&GraphNode> {
        self.graph
            .nodes
            .iter()
            .filter(|node| {
                node.node_type == NodeType::Function
                    && (node.label == function_name || node.id.contains(function_name))
            })
            .collect()
    }

    /// Get recursive call edges
    pub fn get_recursive_calls(&self) -> Vec<&GraphEdge> {
        self.graph
            .edges
            .iter()
            .filter(|edge| edge.edge_type == EdgeType::RecursiveCall)
            .collect()
    }

    /// Get call chain from a function
    pub fn get_call_chain(&self, function_id: &str) -> Result<Vec<CallGraphPath>> {
        let mut paths = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(vec![function_id.to_string()]);

        while let Some(current_path) = queue.pop_front() {
            let current_node = current_path.last().unwrap();

            if visited.contains(current_node) {
                continue;
            }
            visited.insert(current_node.clone());

            // Find all outgoing call edges
            for edge in &self.graph.edges {
                if edge.source == *current_node && edge.edge_type == EdgeType::Calls {
                    let mut new_path = current_path.clone();
                    new_path.push(edge.target.clone());
                    paths.push(CallGraphPath {
                        nodes: new_path.clone(),
                        edges: vec![edge.clone()],
                        length: new_path.len() - 1,
                    });

                    if new_path.len() < 10 {
                        // Prevent infinite recursion
                        queue.push_back(new_path);
                    }
                }
            }
        }

        Ok(paths)
    }

    /// Check if a node matches the filter criteria
    fn matches_node_filter(&self, node: &GraphNode, filter: &NodeFilter) -> Result<bool> {
        // Check node types
        if let Some(ref node_types) = filter.node_types {
            if !node_types.contains(&node.node_type) {
                return Ok(false);
            }
        }

        // Check labels (exact match)
        if let Some(ref labels) = filter.labels {
            if !labels.contains(&node.label) {
                return Ok(false);
            }
        }

        // Check labels (contains)
        if let Some(ref label_contains) = filter.label_contains {
            let node_label = if filter.case_sensitive {
                node.label.clone()
            } else {
                node.label.to_lowercase()
            };

            let mut matches = false;
            for pattern in label_contains {
                let pattern = if filter.case_sensitive {
                    pattern.clone()
                } else {
                    pattern.to_lowercase()
                };

                if node_label.contains(&pattern) {
                    matches = true;
                    break;
                }
            }
            if !matches {
                return Ok(false);
            }
        }

        // Check metadata
        if let Some(ref metadata_filter) = filter.metadata {
            for (key, expected_value) in metadata_filter {
                if let Some(actual_value) = node.metadata.get(key) {
                    if actual_value != expected_value {
                        return Ok(false);
                    }
                } else {
                    return Ok(false);
                }
            }
        }

        // Check module paths
        if let Some(ref module_paths) = filter.module_paths {
            let mut matches = false;
            for path in module_paths {
                if node.id.contains(path) {
                    matches = true;
                    break;
                }
            }
            if !matches {
                return Ok(false);
            }
        }

        // Check function names
        if let Some(ref function_names) = filter.function_names {
            if node.node_type == NodeType::Function {
                let mut matches = false;
                for name in function_names {
                    if node.label == *name || node.id.contains(name) {
                        matches = true;
                        break;
                    }
                }
                if !matches {
                    return Ok(false);
                }
            }
        }

        // Check position bounds
        if let Some((x_min, y_min, x_max, y_max)) = filter.position_bounds {
            if let Some((x, y)) = node.position {
                if x < x_min || x > x_max || y < y_min || y > y_max {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }

        // Check size range
        if let Some((min_size, max_size)) = filter.size_range {
            if let Some(size) = node.size {
                if size < min_size || size > max_size {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }

        // Check colors
        if let Some(ref colors) = filter.colors {
            if let Some(ref color) = node.color {
                if !colors.contains(color) {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Check if an edge matches the filter criteria
    fn matches_edge_filter(&self, edge: &GraphEdge, filter: &EdgeFilter) -> Result<bool> {
        // Check edge types
        if let Some(ref edge_types) = filter.edge_types {
            if !edge_types.contains(&edge.edge_type) {
                return Ok(false);
            }
        }

        // Check source nodes
        if let Some(ref source_nodes) = filter.source_nodes {
            if !source_nodes.contains(&edge.source) {
                return Ok(false);
            }
        }

        // Check target nodes
        if let Some(ref target_nodes) = filter.target_nodes {
            if !target_nodes.contains(&edge.target) {
                return Ok(false);
            }
        }

        // Check weight range
        if let Some((min_weight, max_weight)) = filter.weight_range {
            if edge.weight < min_weight || edge.weight > max_weight {
                return Ok(false);
            }
        }

        // Check labels (exact match)
        if let Some(ref labels) = filter.labels {
            if let Some(ref label) = edge.label {
                if !labels.contains(label) {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }

        // Check labels (contains)
        if let Some(ref label_contains) = filter.label_contains {
            if let Some(ref label) = edge.label {
                let edge_label = if filter.case_sensitive {
                    label.clone()
                } else {
                    label.to_lowercase()
                };

                let mut matches = false;
                for pattern in label_contains {
                    let pattern = if filter.case_sensitive {
                        pattern.clone()
                    } else {
                        pattern.to_lowercase()
                    };

                    if edge_label.contains(&pattern) {
                        matches = true;
                        break;
                    }
                }
                if !matches {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }

        // Check metadata
        if let Some(ref metadata_filter) = filter.metadata {
            for (key, expected_value) in metadata_filter {
                if let Some(actual_value) = edge.metadata.get(key) {
                    if actual_value != expected_value {
                        return Ok(false);
                    }
                } else {
                    return Ok(false);
                }
            }
        }

        // Check recursive only
        if let Some(recursive_only) = filter.recursive_only {
            if recursive_only && edge.edge_type != EdgeType::RecursiveCall {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Check if a node matches search criteria
    fn matches_search_criteria(&self, node: &GraphNode, search: &CallGraphSearch) -> Result<bool> {
        if search.query.is_empty() {
            return Ok(false);
        }

        let query = if search.case_sensitive {
            search.query.clone()
        } else {
            search.query.to_lowercase()
        };

        // Search in labels
        if search.search_labels {
            let node_label = if search.case_sensitive {
                node.label.clone()
            } else {
                node.label.to_lowercase()
            };

            if self.text_matches(&node_label, &query, search.use_regex)? {
                return Ok(true);
            }
        }

        // Search in metadata
        if search.search_metadata {
            for value in node.metadata.values() {
                let value_str = value.to_string();
                let value_str = if search.case_sensitive {
                    value_str
                } else {
                    value_str.to_lowercase()
                };

                if self.text_matches(&value_str, &query, search.use_regex)? {
                    return Ok(true);
                }
            }
        }

        // Search in function names only
        if search.function_names_only && node.node_type == NodeType::Function {
            let node_label = if search.case_sensitive {
                node.label.clone()
            } else {
                node.label.to_lowercase()
            };

            if self.text_matches(&node_label, &query, search.use_regex)? {
                return Ok(true);
            }
        }

        // Search in module paths only
        if search.module_paths_only && node.node_type == NodeType::Module {
            let node_id = if search.case_sensitive {
                node.id.clone()
            } else {
                node.id.to_lowercase()
            };

            if self.text_matches(&node_id, &query, search.use_regex)? {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if an edge matches search criteria
    fn matches_edge_search_criteria(
        &self,
        edge: &GraphEdge,
        search: &CallGraphSearch,
    ) -> Result<bool> {
        if search.query.is_empty() {
            return Ok(false);
        }

        let query = if search.case_sensitive {
            search.query.clone()
        } else {
            search.query.to_lowercase()
        };

        // Search in edge labels
        if search.search_edge_labels {
            if let Some(ref label) = edge.label {
                let edge_label = if search.case_sensitive {
                    label.clone()
                } else {
                    label.to_lowercase()
                };

                if self.text_matches(&edge_label, &query, search.use_regex)? {
                    return Ok(true);
                }
            }
        }

        // Search in edge metadata
        if search.search_edge_metadata {
            for value in edge.metadata.values() {
                let value_str = value.to_string();
                let value_str = if search.case_sensitive {
                    value_str
                } else {
                    value_str.to_lowercase()
                };

                if self.text_matches(&value_str, &query, search.use_regex)? {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Check if text matches query (with regex support)
    fn text_matches(&self, text: &str, query: &str, use_regex: bool) -> Result<bool> {
        if use_regex {
            let regex = regex::Regex::new(query)?;
            Ok(regex.is_match(text))
        } else {
            Ok(text.contains(query))
        }
    }

    /// Find paths between two specific nodes
    fn find_paths_between(
        &self,
        start: &str,
        end: &str,
        path_search: &PathSearch,
    ) -> Result<Vec<CallGraphPath>> {
        let mut paths = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        queue.push_back(vec![start.to_string()]);

        while let Some(current_path) = queue.pop_front() {
            let current_node = current_path.last().unwrap();

            if current_node == end {
                // Found a path to the target
                let edges = self.get_edges_for_path(&current_path)?;
                paths.push(CallGraphPath {
                    nodes: current_path.clone(),
                    edges,
                    length: current_path.len() - 1,
                });

                if !path_search.find_all_paths {
                    break; // Only find shortest path
                }
                continue;
            }

            // Check path length limits
            if let Some(max_length) = path_search.max_length {
                if current_path.len() > max_length {
                    continue;
                }
            }

            // Find next nodes
            for edge in &self.graph.edges {
                if edge.source == *current_node {
                    // Check if edge type is allowed
                    if let Some(ref allowed_types) = path_search.allowed_edge_types {
                        if !allowed_types.contains(&edge.edge_type) {
                            continue;
                        }
                    }

                    // Check if edge type is forbidden
                    if let Some(ref forbidden_types) = path_search.forbidden_edge_types {
                        if forbidden_types.contains(&edge.edge_type) {
                            continue;
                        }
                    }

                    // Check for cycles
                    if !path_search.include_cycles && current_path.contains(&edge.target) {
                        continue;
                    }

                    // Add to queue if not visited or cycles allowed
                    if path_search.include_cycles || !visited.contains(&edge.target) {
                        let mut new_path = current_path.clone();
                        new_path.push(edge.target.clone());
                        queue.push_back(new_path);
                    }
                }
            }

            visited.insert(current_node.clone());
        }

        // Filter by minimum length
        if let Some(min_length) = path_search.min_length {
            paths.retain(|path| path.length >= min_length);
        }

        Ok(paths)
    }

    /// Find all paths from a starting node
    fn find_paths_from(&self, start: &str, path_search: &PathSearch) -> Result<Vec<CallGraphPath>> {
        let mut paths = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        queue.push_back(vec![start.to_string()]);

        while let Some(current_path) = queue.pop_front() {
            let current_node = current_path.last().unwrap();

            // Check if this is a valid path length
            if let Some(min_length) = path_search.min_length {
                if current_path.len() >= min_length {
                    let edges = self.get_edges_for_path(&current_path)?;
                    paths.push(CallGraphPath {
                        nodes: current_path.clone(),
                        edges,
                        length: current_path.len() - 1,
                    });
                }
            }

            // Check path length limits
            if let Some(max_length) = path_search.max_length {
                if current_path.len() >= max_length {
                    continue;
                }
            }

            // Find next nodes
            for edge in &self.graph.edges {
                if edge.source == *current_node {
                    // Check if edge type is allowed
                    if let Some(ref allowed_types) = path_search.allowed_edge_types {
                        if !allowed_types.contains(&edge.edge_type) {
                            continue;
                        }
                    }

                    // Check if edge type is forbidden
                    if let Some(ref forbidden_types) = path_search.forbidden_edge_types {
                        if forbidden_types.contains(&edge.edge_type) {
                            continue;
                        }
                    }

                    // Check for cycles
                    if !path_search.include_cycles && current_path.contains(&edge.target) {
                        continue;
                    }

                    // Add to queue if not visited or cycles allowed
                    if path_search.include_cycles || !visited.contains(&edge.target) {
                        let mut new_path = current_path.clone();
                        new_path.push(edge.target.clone());
                        queue.push_back(new_path);
                    }
                }
            }

            visited.insert(current_node.clone());
        }

        Ok(paths)
    }

    /// Find all paths to a target node
    fn find_paths_to(&self, end: &str, path_search: &PathSearch) -> Result<Vec<CallGraphPath>> {
        let mut paths = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        queue.push_back(vec![end.to_string()]);

        while let Some(current_path) = queue.pop_front() {
            let current_node = current_path.last().unwrap();

            // Check if this is a valid path length
            if let Some(min_length) = path_search.min_length {
                if current_path.len() >= min_length {
                    let edges = self.get_edges_for_path(&current_path)?;
                    paths.push(CallGraphPath {
                        nodes: current_path.clone(),
                        edges,
                        length: current_path.len() - 1,
                    });
                }
            }

            // Check path length limits
            if let Some(max_length) = path_search.max_length {
                if current_path.len() >= max_length {
                    continue;
                }
            }

            // Find previous nodes (reverse direction)
            for edge in &self.graph.edges {
                if edge.target == *current_node {
                    // Check if edge type is allowed
                    if let Some(ref allowed_types) = path_search.allowed_edge_types {
                        if !allowed_types.contains(&edge.edge_type) {
                            continue;
                        }
                    }

                    // Check if edge type is forbidden
                    if let Some(ref forbidden_types) = path_search.forbidden_edge_types {
                        if forbidden_types.contains(&edge.edge_type) {
                            continue;
                        }
                    }

                    // Check for cycles
                    if !path_search.include_cycles && current_path.contains(&edge.source) {
                        continue;
                    }

                    // Add to queue if not visited or cycles allowed
                    if path_search.include_cycles || !visited.contains(&edge.source) {
                        let mut new_path = current_path.clone();
                        new_path.push(edge.source.clone());
                        queue.push_back(new_path);
                    }
                }
            }

            visited.insert(current_node.clone());
        }

        Ok(paths)
    }

    /// Find all possible paths in the graph
    fn find_all_paths(&self, path_search: &PathSearch) -> Result<Vec<CallGraphPath>> {
        let mut all_paths = Vec::new();

        for node in &self.graph.nodes {
            let paths_from_node = self.find_paths_from(&node.id, path_search)?;
            all_paths.extend(paths_from_node);
        }

        Ok(all_paths)
    }

    /// Get edges for a given path
    fn get_edges_for_path(&self, path: &[String]) -> Result<Vec<GraphEdge>> {
        let mut edges = Vec::new();

        for i in 0..path.len() - 1 {
            let source = &path[i];
            let target = &path[i + 1];

            for edge in &self.graph.edges {
                if edge.source == *source && edge.target == *target {
                    edges.push(edge.clone());
                    break;
                }
            }
        }

        Ok(edges)
    }
}

/// Result of a call graph search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraphSearchResult {
    /// Matching nodes
    pub matching_nodes: Vec<GraphNode>,
    /// Matching edges
    pub matching_edges: Vec<GraphEdge>,
    /// Total number of matches
    pub total_matches: usize,
}

/// A path in the call graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraphPath {
    /// Nodes in the path
    pub nodes: Vec<String>,
    /// Edges in the path
    pub edges: Vec<GraphEdge>,
    /// Length of the path
    pub length: usize,
}

impl CallGraphPath {
    /// Get the starting node of the path
    pub fn start_node(&self) -> Option<&String> {
        self.nodes.first()
    }

    /// Get the ending node of the path
    pub fn end_node(&self) -> Option<&String> {
        self.nodes.last()
    }

    /// Check if the path is a cycle
    pub fn is_cycle(&self) -> bool {
        if self.nodes.len() < 2 {
            return false;
        }
        self.nodes.first() == self.nodes.last()
    }
}

impl fmt::Display for CallGraphPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.nodes.join(" -> "))
    }
}

/// Extension trait for CorrelationGraph to add filtering capabilities
pub trait CallGraphFiltering {
    /// Create a filter for this graph
    fn create_filter(&self) -> CallGraphFilter;

    /// Filter nodes with a simple criteria
    fn filter_nodes_by_type(&self, node_type: NodeType) -> Vec<&GraphNode>;

    /// Filter edges by type
    fn filter_edges_by_type(&self, edge_type: EdgeType) -> Vec<&GraphEdge>;

    /// Search for nodes and edges
    fn search_graph(&self, query: &str) -> Result<CallGraphSearchResult>;

    /// Find paths between nodes
    fn find_paths_between(&self, start: &str, end: &str) -> Result<Vec<CallGraphPath>>;
}

impl CallGraphFiltering for CorrelationGraph {
    fn create_filter(&self) -> CallGraphFilter {
        CallGraphFilter::new(self.clone())
    }

    fn filter_nodes_by_type(&self, node_type: NodeType) -> Vec<&GraphNode> {
        self.nodes
            .iter()
            .filter(|node| node.node_type == node_type)
            .collect()
    }

    fn filter_edges_by_type(&self, edge_type: EdgeType) -> Vec<&GraphEdge> {
        self.edges
            .iter()
            .filter(|edge| edge.edge_type == edge_type)
            .collect()
    }

    fn search_graph(&self, query: &str) -> Result<CallGraphSearchResult> {
        let search = CallGraphSearch {
            query: query.to_string(),
            ..Default::default()
        };
        let filter = self.create_filter();
        filter.search(&search)
    }

    fn find_paths_between(&self, start: &str, end: &str) -> Result<Vec<CallGraphPath>> {
        let path_search = PathSearch {
            start_node: Some(start.to_string()),
            end_node: Some(end.to_string()),
            ..Default::default()
        };
        let filter = self.create_filter();
        filter.find_paths(&path_search)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::correlation::{GraphEdge, GraphNode, GraphType};
    use std::collections::HashMap;

    fn create_test_graph() -> CorrelationGraph {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

        // Add nodes
        let main_node = GraphNode {
            id: "func:main.rs:main".to_string(),
            node_type: NodeType::Function,
            label: "main".to_string(),
            metadata: HashMap::new(),
            position: Some((0.0, 0.0)),
            size: Some(1.0),
            color: Some("#ff0000".to_string()),
        };
        graph.add_node(main_node).unwrap();

        let helper_node = GraphNode {
            id: "func:main.rs:helper".to_string(),
            node_type: NodeType::Function,
            label: "helper".to_string(),
            metadata: HashMap::new(),
            position: Some((100.0, 0.0)),
            size: Some(1.0),
            color: Some("#00ff00".to_string()),
        };
        graph.add_node(helper_node).unwrap();

        let module_node = GraphNode {
            id: "file:main.rs".to_string(),
            node_type: NodeType::Module,
            label: "main.rs".to_string(),
            metadata: HashMap::new(),
            position: Some((50.0, 50.0)),
            size: Some(2.0),
            color: Some("#0000ff".to_string()),
        };
        graph.add_node(module_node).unwrap();

        // Add edges
        let call_edge = GraphEdge {
            id: "call1".to_string(),
            source: "func:main.rs:main".to_string(),
            target: "func:main.rs:helper".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: Some("calls".to_string()),
        };
        graph.add_edge(call_edge).unwrap();

        let uses_edge = GraphEdge {
            id: "uses1".to_string(),
            source: "file:main.rs".to_string(),
            target: "func:main.rs:main".to_string(),
            edge_type: EdgeType::Uses,
            weight: 1.0,
            metadata: HashMap::new(),
            label: Some("contains".to_string()),
        };
        graph.add_edge(uses_edge).unwrap();

        graph
    }

    #[test]
    fn test_node_filtering_by_type() {
        let graph = create_test_graph();
        let filter = CallGraphFilter::new(graph);

        let function_nodes = filter.get_nodes_by_type(NodeType::Function);
        assert_eq!(function_nodes.len(), 2);

        let module_nodes = filter.get_nodes_by_type(NodeType::Module);
        assert_eq!(module_nodes.len(), 1);
    }

    #[test]
    fn test_edge_filtering_by_type() {
        let graph = create_test_graph();
        let filter = CallGraphFilter::new(graph);

        let call_edges = filter.get_edges_by_type(EdgeType::Calls);
        assert_eq!(call_edges.len(), 1);

        let uses_edges = filter.get_edges_by_type(EdgeType::Uses);
        assert_eq!(uses_edges.len(), 1);
    }

    #[test]
    fn test_node_filtering_by_label() {
        let graph = create_test_graph();
        let filter = CallGraphFilter::new(graph);

        let node_filter = NodeFilter {
            labels: Some(vec!["main".to_string()]),
            ..Default::default()
        };

        let filtered_nodes = filter.filter_nodes(&node_filter).unwrap();
        assert_eq!(filtered_nodes.len(), 1);
        assert_eq!(filtered_nodes[0].label, "main");
    }

    #[test]
    fn test_search_functionality() {
        let graph = create_test_graph();
        let filter = CallGraphFilter::new(graph);

        let search = CallGraphSearch {
            query: "main".to_string(),
            search_labels: true,
            ..Default::default()
        };

        let result = filter.search(&search).unwrap();
        assert!(result.total_matches > 0);
        assert!(!result.matching_nodes.is_empty());
    }

    #[test]
    fn test_path_finding() {
        let graph = create_test_graph();
        let filter = CallGraphFilter::new(graph);

        let path_search = PathSearch {
            start_node: Some("func:main.rs:main".to_string()),
            end_node: Some("func:main.rs:helper".to_string()),
            ..Default::default()
        };

        let paths = filter.find_paths(&path_search).unwrap();
        assert!(!paths.is_empty());
        assert_eq!(paths[0].length, 1);
    }

    #[test]
    fn test_connected_nodes() {
        let graph = create_test_graph();
        let filter = CallGraphFilter::new(graph);

        let connected = filter.get_connected_nodes("func:main.rs:main").unwrap();
        assert_eq!(connected.len(), 2); // helper function and main.rs module
    }

    #[test]
    fn test_extension_trait() {
        let graph = create_test_graph();

        let function_nodes = graph.filter_nodes_by_type(NodeType::Function);
        assert_eq!(function_nodes.len(), 2);

        let search_result = graph.search_graph("main").unwrap();
        assert!(search_result.total_matches > 0);
    }
}
