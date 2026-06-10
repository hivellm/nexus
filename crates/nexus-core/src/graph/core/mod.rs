//! Core graph data structures - Graph, Node, Edge
//!
//! This module provides high-level graph data structures that wrap the low-level
//! storage records and provide a more user-friendly API for graph operations.

// Bring PropertyValue into scope so that `use super::*` in the test module
// (and any other child of core) picks it up — mirrors the original core.rs layout.
use crate::graph::simple::PropertyValue;

mod edge;
mod graph;
mod ids;
mod node;
mod property_store;
mod stats;

pub use edge::Edge;
pub use graph::Graph;
pub use ids::{EdgeId, NodeId};
pub use node::Node;
pub use stats::GraphStats;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{create_isolated_test_graph, create_test_graph};

    #[test]
    fn test_node_creation() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        assert_eq!(node_id.value(), 0);

        let node = graph.get_node(node_id).unwrap().unwrap();
        assert_eq!(node.id, node_id);
        assert!(node.has_label("Person"));
        assert_eq!(node.labels.len(), 1);
    }

    #[test]
    fn test_node_with_multiple_labels() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph
            .create_node(vec!["Person".to_string(), "Employee".to_string()])
            .unwrap();

        let node = graph.get_node(node_id).unwrap().unwrap();
        assert!(node.has_label("Person"));
        assert!(node.has_label("Employee"));
        assert_eq!(node.labels.len(), 2);
    }

    #[test]
    fn test_node_properties() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        node.set_property(
            "name".to_string(),
            PropertyValue::String("test".to_string()),
        );
        node.set_property("age".to_string(), PropertyValue::Int64(30));

        assert!(node.has_property("name"));
        assert!(node.has_property("age"));
        assert_eq!(node.property_keys().len(), 2);
    }

    #[test]
    fn test_edge_creation() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();

        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();
        assert_eq!(edge_id.value(), 0);

        let edge = graph.get_edge(edge_id).unwrap().unwrap();
        assert_eq!(edge.id, edge_id);
        assert_eq!(edge.source, source_id);
        assert_eq!(edge.target, target_id);
        assert_eq!(edge.relationship_type, "KNOWS");
    }

    #[test]
    fn test_edge_properties() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let mut edge = graph.get_edge(edge_id).unwrap().unwrap();
        edge.set_property("since".to_string(), PropertyValue::Int64(2020));

        assert!(edge.has_property("since"));
        assert_eq!(edge.property_keys().len(), 1);
    }

    #[test]
    fn test_node_deletion() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        assert!(graph.get_node(node_id).unwrap().is_some());

        let deleted = graph.delete_node(node_id).unwrap();
        assert!(deleted);
        assert!(graph.get_node(node_id).unwrap().is_none());
    }

    #[test]
    fn test_edge_deletion() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        assert!(graph.get_edge(edge_id).unwrap().is_some());

        let deleted = graph.delete_edge(edge_id).unwrap();
        assert!(deleted);
        assert!(graph.get_edge(edge_id).unwrap().is_none());
    }

    #[test]
    fn test_get_nodes_by_label() {
        let (graph, _dir) = create_test_graph();

        let _person1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _person2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _company = graph.create_node(vec!["Company".to_string()]).unwrap();

        let person_nodes = graph.get_nodes_by_label("Person").unwrap();
        assert_eq!(person_nodes.len(), 2);

        let company_nodes = graph.get_nodes_by_label("Company").unwrap();
        assert_eq!(company_nodes.len(), 1);
    }

    #[test]
    fn test_get_edges_by_type() {
        let (graph, _dir) = create_test_graph();

        let person1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let person2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let company = graph.create_node(vec!["Company".to_string()]).unwrap();

        let _knows_edge = graph
            .create_edge(person1, person2, "KNOWS".to_string())
            .unwrap();
        let _works_edge = graph
            .create_edge(person1, company, "WORKS_AT".to_string())
            .unwrap();

        let knows_edges = graph.get_edges_by_type("KNOWS").unwrap();
        assert_eq!(knows_edges.len(), 1);

        let works_edges = graph.get_edges_by_type("WORKS_AT").unwrap();
        assert_eq!(works_edges.len(), 1);
    }

    #[test]
    fn test_get_edges_for_node() {
        let (graph, _dir) = create_test_graph();

        let person1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let person2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let person3 = graph.create_node(vec!["Person".to_string()]).unwrap();

        let _edge1 = graph
            .create_edge(person1, person2, "KNOWS".to_string())
            .unwrap();
        let _edge2 = graph
            .create_edge(person1, person3, "KNOWS".to_string())
            .unwrap();

        let edges = graph.get_edges_for_node(person1).unwrap();
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn test_graph_stats() {
        let (graph, _dir) = create_test_graph();

        let _person1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _person2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _edge = graph
            .create_edge(NodeId::new(0), NodeId::new(1), "KNOWS".to_string())
            .unwrap();

        let stats = graph.stats().unwrap();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.total_edges, 1);
    }

    #[test]
    fn test_node_label_operations() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        // Add label
        node.add_label("Employee".to_string());
        assert!(node.has_label("Employee"));
        assert_eq!(node.labels.len(), 2);

        // Remove label
        let removed = node.remove_label("Person");
        assert!(removed);
        assert!(!node.has_label("Person"));
        assert!(node.has_label("Employee"));
        assert_eq!(node.labels.len(), 1);

        // Try to remove non-existent label
        let not_removed = node.remove_label("NonExistent");
        assert!(!not_removed);
    }

    #[test]
    fn test_edge_other_end() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let edge = graph.get_edge(edge_id).unwrap().unwrap();

        assert_eq!(edge.other_end(source_id), Some(target_id));
        assert_eq!(edge.other_end(target_id), Some(source_id));
        assert_eq!(edge.other_end(NodeId::new(999)), None);
    }

    #[test]
    fn test_node_property_operations() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        // Set properties
        node.set_property(
            "name".to_string(),
            PropertyValue::String("test".to_string()),
        );
        node.set_property("age".to_string(), PropertyValue::Int64(30));

        // Check properties
        assert!(node.has_property("name"));
        assert!(node.has_property("age"));
        assert_eq!(node.property_keys().len(), 2);

        // Get property
        let age = node.get_property("age").unwrap();
        assert_eq!(age, &PropertyValue::Int64(30));

        // Remove property
        let removed = node.remove_property("age");
        assert_eq!(removed, Some(PropertyValue::Int64(30)));
        assert!(!node.has_property("age"));
        assert!(node.has_property("name"));
    }

    #[test]
    fn test_edge_property_operations() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let mut edge = graph.get_edge(edge_id).unwrap().unwrap();

        // Set properties
        edge.set_property("since".to_string(), PropertyValue::Int64(2020));
        edge.set_property("strength".to_string(), PropertyValue::Float64(0.8));

        // Check properties
        assert!(edge.has_property("since"));
        assert!(edge.has_property("strength"));
        assert_eq!(edge.property_keys().len(), 2);

        // Get property
        let since = edge.get_property("since").unwrap();
        assert_eq!(since, &PropertyValue::Int64(2020));

        // Remove property
        let removed = edge.remove_property("strength");
        assert_eq!(removed, Some(PropertyValue::Float64(0.8)));
        assert!(!edge.has_property("strength"));
        assert!(edge.has_property("since"));
    }

    #[test]
    fn test_property_chain_traversal() {
        let (graph, _dir) = create_test_graph();

        // Create a node with properties
        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        // Set multiple properties
        node.set_property(
            "name".to_string(),
            PropertyValue::String("Alice".to_string()),
        );
        node.set_property("age".to_string(), PropertyValue::Int64(30));
        node.set_property("active".to_string(), PropertyValue::Bool(true));

        // Update the node to store properties
        graph.update_node(node).unwrap();

        // Retrieve the node and verify properties are loaded from the chain
        let retrieved_node = graph.get_node(node_id).unwrap().unwrap();

        // The properties should be loaded from the property chain
        assert!(retrieved_node.has_property("name"));
        assert!(retrieved_node.has_property("age"));
        assert!(retrieved_node.has_property("active"));

        assert_eq!(
            retrieved_node.get_property("name"),
            Some(&PropertyValue::String("Alice".to_string()))
        );
        assert_eq!(
            retrieved_node.get_property("age"),
            Some(&PropertyValue::Int64(30))
        );
        assert_eq!(
            retrieved_node.get_property("active"),
            Some(&PropertyValue::Bool(true))
        );
    }

    #[test]
    fn test_node_is_empty() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec![]).unwrap();
        let node = graph.get_node(node_id).unwrap().unwrap();
        assert!(node.is_empty());

        let mut node_with_label = graph.get_node(node_id).unwrap().unwrap();
        node_with_label.add_label("Person".to_string());
        assert!(!node_with_label.is_empty());
    }

    // Uses the shared-catalog `create_test_graph` helper instead of
    // `create_isolated_test_graph` — opening a fresh LMDB env here
    // used to push the process past Windows' TLS-slot ceiling and
    // surface as `Database(Mdb(TlsFull))`. This test only inspects
    // the `edge.is_empty()` predicate on its own fresh node +
    // relationship ids; it does not depend on the catalog label id
    // space being empty, so the shared catalog is safe.
    #[test]
    fn test_edge_is_empty() {
        let (graph, _dir) = create_test_graph();

        let source_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let target_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let edge_id = graph
            .create_edge(source_id, target_id, "KNOWS".to_string())
            .unwrap();

        let edge = graph.get_edge(edge_id).unwrap().unwrap();
        assert!(edge.is_empty());

        let mut edge_with_props = graph.get_edge(edge_id).unwrap().unwrap();
        edge_with_props.set_property("since".to_string(), PropertyValue::Int64(2020));
        assert!(!edge_with_props.is_empty());
    }

    #[test]
    fn test_clear_cache() {
        // Shared-catalog helper — see `test_edge_is_empty` above for
        // the TLS rationale. This test only checks cache state on
        // its own newly-created nodes + edge.
        let (graph, _dir) = create_test_graph();

        let node_id1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let node_id2 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _edge_id = graph
            .create_edge(node_id1, node_id2, "KNOWS".to_string())
            .unwrap();

        // Verify cache has entries
        assert!(!graph.node_cache.read().is_empty());
        assert!(!graph.edge_cache.read().is_empty());

        // Clear cache
        graph.clear_cache();

        // Verify cache is empty
        assert!(graph.node_cache.read().is_empty());
        assert!(graph.edge_cache.read().is_empty());
    }
}
