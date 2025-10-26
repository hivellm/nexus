//! Graph Export Functionality
//!
//! Export graphs to multiple formats: JSON, GraphML, GEXF, DOT

use crate::Result;
use crate::graph_correlation::{CorrelationGraph, EdgeType, NodeType};
use serde::Serialize;
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
    use crate::graph_correlation::{GraphEdge, GraphNode, GraphType};

    #[test]
    fn test_export_json() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let result = export_json(&graph);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Test"));
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("<test>"), "&lt;test&gt;");
        assert_eq!(escape_xml("a&b"), "a&amp;b");
    }

    #[test]
    fn test_escape_dot() {
        assert_eq!(escape_dot("\"test\""), "\\\"test\\\"");
    }
}
