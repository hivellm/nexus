//! MERGE clause execution

use nexus_core::{executor::parser, Engine};
use serde_json;

use crate::api::cypher::{CypherResponse, utils::property_map_to_json};

/// Execute MERGE clause
/// Note: Currently simplified - just creates the node without checking for existing
/// TODO: Implement proper match-or-create semantics with property matching
pub fn execute_merge(
    engine: &mut Engine,
    merge_clause: &parser::MergeClause,
) -> Result<(), CypherResponse> {
    for element in &merge_clause.pattern.elements {
        if let parser::PatternElement::Node(node_pattern) = element {
            let labels = node_pattern.labels.clone();
            let properties = property_map_to_json(&node_pattern.properties);

            // For now, just create the node (simplified MERGE without property matching)
            match engine.create_node(labels, properties) {
                Ok(_node_id) => {
                    tracing::info!("Node merged successfully via Engine");
                }
                Err(e) => {
                    tracing::error!("Failed to merge node: {}", e);
                    return Err(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: 0,
                        error: Some(format!("Failed to merge node: {}", e)),
                    });
                }
            }
        }
    }
    Ok(())
}

