//! GraphQL schema generation from database catalog

use crate::NexusServer;
use crate::api::graphql::types::*;
use async_graphql::{Context, Object, Result as GQLResult};
use nexus_core::executor::Query;
use std::collections::HashMap;
use std::sync::Arc;

/// Root query type for GraphQL
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get a node by ID
    async fn node(&self, ctx: &Context<'_>, id: String) -> GQLResult<Option<Node>> {
        let server = ctx.data::<Arc<NexusServer>>()?;

        // Parse node ID
        let node_id: u64 = id.parse().map_err(|_| "Invalid node ID format")?;

        // Execute Cypher query to get node
        let query_str = format!(
            "MATCH (n) WHERE id(n) = {} RETURN id(n), labels(n), properties(n)",
            node_id
        );

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        if result.rows.is_empty() {
            return Ok(None);
        }

        // Convert result to Node
        let row = &result.rows[0];
        let node = convert_row_to_node(node_id, &row.values)?;

        Ok(Some(node))
    }

    /// Get multiple nodes with optional filtering
    async fn nodes(
        &self,
        ctx: &Context<'_>,
        filter: Option<NodeFilterInput>,
    ) -> GQLResult<Vec<Node>> {
        let server = ctx.data::<Arc<NexusServer>>()?;
        let filter = filter.unwrap_or_default();

        // Build Cypher query from filter
        let query_str = build_nodes_query(&filter);

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        // Convert results to nodes
        let mut nodes = Vec::new();
        for row in &result.rows {
            if let Some(node) = row_to_node(&row.values)? {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    /// Get relationships for a node
    async fn relationships(
        &self,
        ctx: &Context<'_>,
        node_id: String,
        direction: Option<String>, // "IN", "OUT", or "BOTH"
    ) -> GQLResult<Vec<Relationship>> {
        let server = ctx.data::<Arc<NexusServer>>()?;

        let node_id: u64 = node_id.parse().map_err(|_| "Invalid node ID format")?;

        let direction = direction.unwrap_or_else(|| "BOTH".to_string());

        // Build query based on direction
        let query_str = match direction.to_uppercase().as_str() {
            "OUT" => format!(
                "MATCH (n)-[r]->(m) WHERE id(n) = {} RETURN id(r), type(r), id(n), id(m), properties(r)",
                node_id
            ),
            "IN" => format!(
                "MATCH (n)<-[r]-(m) WHERE id(n) = {} RETURN id(r), type(r), id(m), id(n), properties(r)",
                node_id
            ),
            _ => format!(
                "MATCH (n)-[r]-(m) WHERE id(n) = {} RETURN id(r), type(r), id(n), id(m), properties(r)",
                node_id
            ),
        };

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        // Convert results to relationships
        let mut relationships = Vec::new();
        for row in &result.rows {
            if let Some(rel) = row_to_relationship(&row.values)? {
                relationships.push(rel);
            }
        }

        Ok(relationships)
    }

    /// Execute a raw Cypher query (advanced usage)
    async fn cypher(&self, ctx: &Context<'_>, query_str: String) -> GQLResult<CypherResult> {
        let server = ctx.data::<Arc<NexusServer>>()?;

        let start = std::time::Instant::now();

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        let elapsed = start.elapsed().as_millis() as i64;

        Ok(CypherResult {
            columns: result.columns,
            rows: result
                .rows
                .iter()
                .map(|row| {
                    row.values
                        .iter()
                        .map(|v| json_to_property_value(v))
                        .collect()
                })
                .collect(),
            execution_time_ms: elapsed,
        })
    }
}

/// Result of a raw Cypher query
#[derive(Debug, Clone)]
pub struct CypherResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<PropertyValue>>,
    pub execution_time_ms: i64,
}

#[Object]
impl CypherResult {
    async fn columns(&self) -> &Vec<String> {
        &self.columns
    }

    async fn rows(&self) -> &Vec<Vec<PropertyValue>> {
        &self.rows
    }

    async fn execution_time_ms(&self) -> i64 {
        self.execution_time_ms
    }
}

// Helper functions

fn build_nodes_query(filter: &NodeFilterInput) -> String {
    let mut query = String::from("MATCH (n)");

    // Add label filter
    if let Some(labels) = &filter.labels {
        if !labels.is_empty() {
            let label_str = labels
                .iter()
                .map(|l| format!(":{}", l))
                .collect::<Vec<_>>()
                .join("|");
            query.push_str(&format!(":{}", label_str));
        }
    }

    // Add WHERE clause for properties
    if let Some(props) = &filter.properties {
        if !props.is_empty() {
            query.push_str(" WHERE ");
            let conditions: Vec<String> = props
                .iter()
                .map(|(k, v)| format!("n.{} = {}", k, property_value_to_cypher(v)))
                .collect();
            query.push_str(&conditions.join(" AND "));
        }
    }

    query.push_str(" RETURN id(n), labels(n), properties(n)");

    // Add ORDER BY
    if let Some(order_by) = &filter.order_by {
        let direction = if filter.order_desc.unwrap_or(false) {
            "DESC"
        } else {
            "ASC"
        };
        query.push_str(&format!(" ORDER BY n.{} {}", order_by, direction));
    }

    // Add SKIP/LIMIT
    if let Some(skip) = filter.skip {
        query.push_str(&format!(" SKIP {}", skip));
    }
    if let Some(limit) = filter.limit {
        query.push_str(&format!(" LIMIT {}", limit));
    }

    query
}

fn property_value_to_cypher(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Null => "null".to_string(),
        PropertyValue::Boolean(b) => b.to_string(),
        PropertyValue::Integer(i) => i.to_string(),
        PropertyValue::Float(f) => f.to_string(),
        PropertyValue::String(s) => format!("'{}'", s.replace('\'', "\\'")),
        PropertyValue::List(_) => "[]".to_string(), // Simplified
        PropertyValue::Map(_) => "{}".to_string(),  // Simplified
    }
}

fn convert_row_to_node(node_id: u64, row: &[serde_json::Value]) -> GQLResult<Node> {
    use async_graphql::ID;
    use std::collections::HashMap;

    // Assuming row format: [node_data, labels, properties]
    let labels = if row.len() > 1 {
        serde_json::from_value(row[1].clone()).unwrap_or_default()
    } else {
        Vec::new()
    };

    let properties: HashMap<String, PropertyValue> = if row.len() > 2 {
        serde_json::from_value(row[2].clone()).unwrap_or_default()
    } else {
        HashMap::new()
    };

    Ok(Node {
        id: ID::from(node_id.to_string()),
        labels,
        properties,
    })
}

fn row_to_node(row: &[serde_json::Value]) -> GQLResult<Option<Node>> {
    if row.is_empty() {
        return Ok(None);
    }

    // Extract node ID from first column
    let node_id: u64 =
        serde_json::from_value(row[0].clone()).map_err(|_| "Failed to parse node ID")?;

    Ok(Some(convert_row_to_node(node_id, row)?))
}

fn row_to_relationship(row: &[serde_json::Value]) -> GQLResult<Option<Relationship>> {
    if row.len() < 4 {
        return Ok(None);
    }

    use async_graphql::ID;
    use std::collections::HashMap;

    // Assuming row format: [rel_data, type, from_id, to_id, properties]
    let rel_id: u64 = serde_json::from_value(row[0].clone()).unwrap_or(0);
    let rel_type: String = serde_json::from_value(row[1].clone()).unwrap_or_default();
    let from_id: u64 = serde_json::from_value(row[2].clone()).unwrap_or(0);
    let to_id: u64 = serde_json::from_value(row[3].clone()).unwrap_or(0);
    let properties: HashMap<String, PropertyValue> = if row.len() > 4 {
        serde_json::from_value(row[4].clone()).unwrap_or_default()
    } else {
        HashMap::new()
    };

    Ok(Some(Relationship {
        id: ID::from(rel_id.to_string()),
        rel_type,
        from: ID::from(from_id.to_string()),
        to: ID::from(to_id.to_string()),
        properties,
    }))
}

fn json_to_property_value(value: &serde_json::Value) -> PropertyValue {
    match value {
        serde_json::Value::Null => PropertyValue::Null,
        serde_json::Value::Bool(b) => PropertyValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                PropertyValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                PropertyValue::Float(f)
            } else {
                PropertyValue::Null
            }
        }
        serde_json::Value::String(s) => PropertyValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            PropertyValue::List(arr.iter().map(json_to_property_value).collect())
        }
        serde_json::Value::Object(obj) => PropertyValue::Map(
            obj.iter()
                .map(|(k, v)| (k.clone(), json_to_property_value(v)))
                .collect(),
        ),
    }
}
