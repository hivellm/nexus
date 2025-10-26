//! Graph Export Functionality
//!
//! Export graphs to multiple formats: JSON, GraphML, GEXF, DOT

use crate::Result;
use crate::graph_correlation::{CorrelationGraph, EdgeType, NodeType};
use std::fmt::Write;

/// Graph export format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    GraphML,
    GEXF,
    DOT,
}

/// Export graph to specified format
pub fn export_graph(graph: &CorrelationGraph, format: ExportFormat) -> Result<String> {
    match format {
        ExportFormat::Json => export_json(graph),
        ExportFormat::GraphML => export_graphml(graph),
        ExportFormat::GEXF => export_gexf(graph),
        ExportFormat::DOT => export_dot(graph),
    }
}

fn export_json(graph: &CorrelationGraph) -> Result<String> {
    serde_json::to_string_pretty(graph)
        .map_err(|e| crate::Error::GraphCorrelation(format!("JSON export failed: {}", e)))
}

fn export_graphml(graph: &CorrelationGraph) -> Result<String> {
    let mut output = String::new();

    writeln!(&mut output, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>").unwrap();
    writeln!(
        &mut output,
        "<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">"
    )
    .unwrap();
    writeln!(
        &mut output,
        "  <graph id=\"{}\" edgedefault=\"directed\">",
        graph.name
    )
    .unwrap();

    // Export nodes
    for node in &graph.nodes {
        writeln!(&mut output, "    <node id=\"{}\">", escape_xml(&node.id)).unwrap();
        writeln!(
            &mut output,
            "      <data key=\"label\">{}</data>",
            escape_xml(&node.label)
        )
        .unwrap();
        writeln!(
            &mut output,
            "      <data key=\"type\">{:?}</data>",
            node.node_type
        )
        .unwrap();
        writeln!(&mut output, "    </node>").unwrap();
    }

    // Export edges
    for edge in &graph.edges {
        writeln!(
            &mut output,
            "    <edge source=\"{}\" target=\"{}\">",
            escape_xml(&edge.source),
            escape_xml(&edge.target)
        )
        .unwrap();
        writeln!(
            &mut output,
            "      <data key=\"type\">{:?}</data>",
            edge.edge_type
        )
        .unwrap();
        writeln!(
            &mut output,
            "      <data key=\"weight\">{}</data>",
            edge.weight
        )
        .unwrap();
        writeln!(&mut output, "    </edge>").unwrap();
    }

    writeln!(&mut output, "  </graph>").unwrap();
    writeln!(&mut output, "</graphml>").unwrap();

    Ok(output)
}

fn export_gexf(graph: &CorrelationGraph) -> Result<String> {
    let mut output = String::new();

    writeln!(&mut output, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>").unwrap();
    writeln!(
        &mut output,
        "<gexf xmlns=\"http://www.gexf.net/1.2draft\" version=\"1.2\">"
    )
    .unwrap();
    writeln!(
        &mut output,
        "  <graph mode=\"static\" defaultedgetype=\"directed\">"
    )
    .unwrap();
    writeln!(&mut output, "    <nodes>").unwrap();

    for node in &graph.nodes {
        writeln!(
            &mut output,
            "      <node id=\"{}\" label=\"{}\"/>",
            escape_xml(&node.id),
            escape_xml(&node.label)
        )
        .unwrap();
    }

    writeln!(&mut output, "    </nodes>").unwrap();
    writeln!(&mut output, "    <edges>").unwrap();

    for edge in &graph.edges {
        writeln!(
            &mut output,
            "      <edge source=\"{}\" target=\"{}\" weight=\"{}\"/>",
            escape_xml(&edge.source),
            escape_xml(&edge.target),
            edge.weight
        )
        .unwrap();
    }

    writeln!(&mut output, "    </edges>").unwrap();
    writeln!(&mut output, "  </graph>").unwrap();
    writeln!(&mut output, "</gexf>").unwrap();

    Ok(output)
}

fn export_dot(graph: &CorrelationGraph) -> Result<String> {
    let mut output = String::new();

    writeln!(&mut output, "digraph \"{}\" {{", escape_dot(&graph.name)).unwrap();
    writeln!(&mut output, "  rankdir=TB;").unwrap();
    writeln!(&mut output, "  node [shape=box];").unwrap();

    // Export nodes
    for node in &graph.nodes {
        let color = match node.node_type {
            NodeType::Function => "#3498db",
            NodeType::Module => "#2ecc71",
            NodeType::Class => "#9b59b6",
            NodeType::Variable => "#f39c12",
            NodeType::API => "#e74c3c",
        };

        writeln!(
            &mut output,
            "  \"{}\" [label=\"{}\" color=\"{}\" style=filled fillcolor=\"{}33\"];",
            escape_dot(&node.id),
            escape_dot(&node.label),
            color,
            color
        )
        .unwrap();
    }

    // Export edges
    for edge in &graph.edges {
        let style = match edge.edge_type {
            EdgeType::Calls => "solid",
            EdgeType::Imports => "dashed",
            EdgeType::Depends => "dotted",
            _ => "solid",
        };

        writeln!(
            &mut output,
            "  \"{}\" -> \"{}\" [style={} label=\"{:?}\"];",
            escape_dot(&edge.source),
            escape_dot(&edge.target),
            style,
            edge.edge_type
        )
        .unwrap();
    }

    writeln!(&mut output, "}}").unwrap();

    Ok(output)
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn escape_dot(s: &str) -> String {
    s.replace('"', "\\\"").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_correlation::{EdgeType, GraphEdge, GraphNode, GraphType, NodeType};
    use std::collections::HashMap;

    fn create_test_graph() -> CorrelationGraph {
        let nodes = vec![
            GraphNode {
                id: "node1".to_string(),
                label: "Node 1".to_string(),
                node_type: NodeType::Function,
                properties: HashMap::new(),
            },
            GraphNode {
                id: "node2".to_string(),
                label: "Node 2".to_string(),
                node_type: NodeType::Function,
                properties: HashMap::new(),
            },
        ];

        let edges = vec![GraphEdge {
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            properties: HashMap::new(),
        }];

        CorrelationGraph {
            graph_type: GraphType::Call,
            nodes,
            edges,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_export_json_empty_graph() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let result = export_json(&graph);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Test"));
    }

    #[test]
    fn test_export_json_with_nodes_and_edges() {
        let graph = create_test_graph();
        let result = export_json(&graph);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("node1"));
        assert!(json.contains("node2"));
    }

    #[test]
    fn test_export_graphml_empty() {
        let graph = CorrelationGraph::new(GraphType::Dependency, "Empty".to_string());
        let result = export_graphml(&graph);
        assert!(result.is_ok());
        let xml = result.unwrap();
        assert!(xml.contains("<?xml"));
        assert!(xml.contains("<graphml"));
    }

    #[test]
    fn test_export_graphml_with_data() {
        let graph = create_test_graph();
        let result = export_graphml(&graph);
        assert!(result.is_ok());
        let xml = result.unwrap();
        assert!(xml.contains("<node"));
        assert!(xml.contains("<edge"));
        assert!(xml.contains("node1"));
    }

    #[test]
    fn test_export_gexf_empty() {
        let graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());
        let result = export_gexf(&graph);
        assert!(result.is_ok());
        let xml = result.unwrap();
        assert!(xml.contains("<?xml"));
        assert!(xml.contains("<gexf"));
    }

    #[test]
    fn test_export_gexf_with_data() {
        let graph = create_test_graph();
        let result = export_gexf(&graph);
        assert!(result.is_ok());
        let xml = result.unwrap();
        assert!(xml.contains("<node"));
        assert!(xml.contains("<edge"));
    }

    #[test]
    fn test_export_dot_empty() {
        let graph = CorrelationGraph::new(GraphType::Component, "Test".to_string());
        let result = export_dot(&graph);
        assert!(result.is_ok());
        let dot = result.unwrap();
        assert!(dot.contains("digraph"));
    }

    #[test]
    fn test_export_dot_with_data() {
        let graph = create_test_graph();
        let result = export_dot(&graph);
        assert!(result.is_ok());
        let dot = result.unwrap();
        assert!(dot.contains("node1"));
        assert!(dot.contains("node2"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_export_graph_json_format() {
        let graph = create_test_graph();
        let result = export_graph(&graph, ExportFormat::JSON);
        assert!(result.is_ok());
    }

    #[test]
    fn test_export_graph_graphml_format() {
        let graph = create_test_graph();
        let result = export_graph(&graph, ExportFormat::GraphML);
        assert!(result.is_ok());
    }

    #[test]
    fn test_export_graph_gexf_format() {
        let graph = create_test_graph();
        let result = export_graph(&graph, ExportFormat::GEXF);
        assert!(result.is_ok());
    }

    #[test]
    fn test_export_graph_dot_format() {
        let graph = create_test_graph();
        let result = export_graph(&graph, ExportFormat::DOT);
        assert!(result.is_ok());
    }

    #[test]
    fn test_escape_xml_basic() {
        assert_eq!(escape_xml("<test>"), "&lt;test&gt;");
        assert_eq!(escape_xml("a&b"), "a&amp;b");
    }

    #[test]
    fn test_escape_xml_quotes() {
        assert_eq!(escape_xml("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(escape_xml("'single'"), "&apos;single&apos;");
    }

    #[test]
    fn test_escape_xml_all_chars() {
        assert_eq!(escape_xml("<>&\"'"), "&lt;&gt;&amp;&quot;&apos;");
    }

    #[test]
    fn test_escape_dot_basic() {
        assert_eq!(escape_dot("\"test\""), "\\\"test\\\"");
    }

    #[test]
    fn test_escape_dot_empty() {
        assert_eq!(escape_dot(""), "");
    }

    #[test]
    fn test_escape_dot_no_quotes() {
        assert_eq!(escape_dot("test"), "test");
    }

    #[test]
    fn test_export_format_display() {
        assert_eq!(format!("{:?}", ExportFormat::JSON), "JSON");
        assert_eq!(format!("{:?}", ExportFormat::GraphML), "GraphML");
        assert_eq!(format!("{:?}", ExportFormat::GEXF), "GEXF");
        assert_eq!(format!("{:?}", ExportFormat::DOT), "DOT");
    }
}
