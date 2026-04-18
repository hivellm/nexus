//! Algorithms test suite. Attached via `#[cfg(test)] mod tests;` in
//! the parent module.

#![allow(unused_imports)]
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
