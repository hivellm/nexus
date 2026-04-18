use nexus_core::graph::correlation::{
    CorrelationGraph, EdgeType, GraphNode, GraphEdge, NodeType, RecursiveCallConfig
};
use std::collections::HashMap;

fn main() {
    let mut graph = CorrelationGraph::new(nexus_core::graph::correlation::GraphType::Call, "Debug Test".to_string());
    
    // Add factorial node
    let node = GraphNode {
        id: "func:factorial".to_string(),
        node_type: NodeType::Function,
        label: "factorial".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    graph.add_node(node).unwrap();
    
    // Add self-loop edge
    let edge = GraphEdge {
        id: "edge:factorial->factorial".to_string(),
        source: "func:factorial".to_string(),
        target: "func:factorial".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: Some("calls".to_string()),
    };
    graph.add_edge(edge).unwrap();
    
    let config = RecursiveCallConfig::default();
    let recursive_info = graph.detect_recursive_calls(&config).unwrap();
    
    tracing::info!("Recursive info: {:?}", recursive_info);
    
    if let Some(info) = recursive_info.get("func:factorial") {
        tracing::info!("Factorial is recursive: {}", info.is_recursive);
        tracing::info!("Direct recursion: {}", info.direct_recursion);
        tracing::info!("Indirect recursion: {}", info.indirect_recursion);
        tracing::info!("Recursion type: {:?}", info.recursion_type);
        tracing::info!("Cycle functions: {:?}", info.cycle_functions);
    }
}
