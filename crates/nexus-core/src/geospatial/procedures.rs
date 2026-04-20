//! Geospatial procedures for CALL statements
//!
//! This module provides geospatial procedures that can be called via Cypher CALL statements.
//!
//! Example usage:
//! ```cypher
//! CALL spatial.withinBBox({minX: 0, minY: 0, maxX: 10, maxY: 10}, 'location')
//! YIELD node
//! RETURN node
//! ```
//!
//! ```cypher
//! CALL spatial.withinDistance(point({x: 5, y: 5}), 10.0, 'location')
//! YIELD node, distance
//! RETURN node, distance
//! ```

use crate::geospatial::Point;
use crate::graph::algorithms::Graph;
use crate::graph::procedures::{
    GraphProcedure, ParameterType, ProcedureParameter, ProcedureResult,
};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Bounding box structure
#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BoundingBox {
    pub fn contains(&self, point: &Point) -> bool {
        // For WGS84, we need to handle wrap-around, but for simplicity, assume standard ranges
        point.x >= self.min_x
            && point.x <= self.max_x
            && point.y >= self.min_y
            && point.y <= self.max_y
    }
}

/// withinBBox procedure - find nodes with points within a bounding box
pub struct WithinBBoxProcedure;

impl GraphProcedure for WithinBBoxProcedure {
    fn name(&self) -> &str {
        "spatial.withinBBox"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "bbox".to_string(),
                param_type: ParameterType::Map,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "property".to_string(),
                param_type: ParameterType::String,
                required: true,
                default: None,
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        // Parse bounding box from args
        let bbox_map = args
            .get("bbox")
            .and_then(|v| v.as_object())
            .ok_or_else(|| Error::CypherSyntax("bbox must be a map".to_string()))?;

        let min_x = bbox_map
            .get("minX")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| Error::CypherSyntax("bbox.minX must be a number".to_string()))?;
        let min_y = bbox_map
            .get("minY")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| Error::CypherSyntax("bbox.minY must be a number".to_string()))?;
        let max_x = bbox_map
            .get("maxX")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| Error::CypherSyntax("bbox.maxX must be a number".to_string()))?;
        let max_y = bbox_map
            .get("maxY")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| Error::CypherSyntax("bbox.maxY must be a number".to_string()))?;

        let _bbox = BoundingBox {
            min_x,
            min_y,
            max_x,
            max_y,
        };

        let _property_name = args
            .get("property")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::CypherSyntax("property must be a string".to_string()))?;

        // For now, return empty results since we need to integrate with the actual graph data
        // In a full implementation, we would:
        // 1. Iterate through all nodes in the graph
        // 2. Extract the Point property from each node
        // 3. Check if the point is within the bounding box
        // 4. Return matching nodes
        let _ = graph; // Suppress unused warning

        Ok(ProcedureResult {
            columns: vec!["node".to_string()],
            rows: vec![],
        })
    }
}

/// withinDistance procedure - find nodes with points within a specific distance
pub struct WithinDistanceProcedure;

impl GraphProcedure for WithinDistanceProcedure {
    fn name(&self) -> &str {
        "spatial.withinDistance"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "point".to_string(),
                param_type: ParameterType::Map,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "distance".to_string(),
                param_type: ParameterType::Float,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "property".to_string(),
                param_type: ParameterType::String,
                required: true,
                default: None,
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        // Parse point from args
        let point_val = args
            .get("point")
            .ok_or_else(|| Error::CypherSyntax("point parameter required".to_string()))?;

        let _center_point = if let Value::Object(_) = point_val {
            Point::from_json_value(point_val)
                .map_err(|e| Error::CypherSyntax(format!("Invalid point: {}", e)))?
        } else {
            return Err(Error::CypherSyntax(
                "point must be a Point object".to_string(),
            ));
        };

        let _max_distance = args
            .get("distance")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| Error::CypherSyntax("distance must be a number".to_string()))?;

        let _property_name = args
            .get("property")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::CypherSyntax("property must be a string".to_string()))?;

        // For now, return empty results since we need to integrate with the actual graph data
        // In a full implementation, we would:
        // 1. Iterate through all nodes in the graph
        // 2. Extract the Point property from each node
        // 3. Calculate distance from center_point
        // 4. If distance <= max_distance, include the node
        // 5. Return matching nodes with their distances
        let _ = graph; // Suppress unused warning

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "distance".to_string()],
            rows: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geospatial::CoordinateSystem;

    #[test]
    fn test_bounding_box_contains() {
        let bbox = BoundingBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 10.0,
            max_y: 10.0,
        };

        let p1 = Point::new_2d(5.0, 5.0, CoordinateSystem::Cartesian);
        let p2 = Point::new_2d(15.0, 5.0, CoordinateSystem::Cartesian);
        let p3 = Point::new_2d(-1.0, 5.0, CoordinateSystem::Cartesian);

        assert!(bbox.contains(&p1));
        assert!(!bbox.contains(&p2));
        assert!(!bbox.contains(&p3));
    }

    #[test]
    fn test_within_bbox_procedure_signature() {
        let proc = WithinBBoxProcedure;
        assert_eq!(proc.name(), "spatial.withinBBox");
        let sig = proc.signature();
        assert_eq!(sig.len(), 2);
        assert_eq!(sig[0].name, "bbox");
        assert_eq!(sig[1].name, "property");
    }

    #[test]
    fn test_within_distance_procedure_signature() {
        let proc = WithinDistanceProcedure;
        assert_eq!(proc.name(), "spatial.withinDistance");
        let sig = proc.signature();
        assert_eq!(sig.len(), 3);
        assert_eq!(sig[0].name, "point");
        assert_eq!(sig[1].name, "distance");
        assert_eq!(sig[2].name, "property");
    }
}
