//! GraphQL mutations for creating, updating, and deleting graph data

use crate::NexusServer;
use crate::api::graphql::types::*;
use async_graphql::{Context, ID, InputObject, Object, Result as GQLResult};
use nexus_core::executor::Query;
use std::collections::HashMap;
use std::sync::Arc;

/// Root mutation type for GraphQL
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Create a new node
    async fn create_node(
        &self,
        ctx: &Context<'_>,
        labels: Vec<String>,
        properties: Option<std::collections::HashMap<String, PropertyValue>>,
    ) -> GQLResult<Node> {
        let server = ctx.data::<Arc<NexusServer>>()?;

        // Build CREATE query
        let label_str = if labels.is_empty() {
            String::new()
        } else {
            format!(":{}", labels.join(":"))
        };

        let props_str = if let Some(props) = &properties {
            if props.is_empty() {
                String::new()
            } else {
                let prop_pairs: Vec<String> = props
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, property_value_to_cypher(v)))
                    .collect();
                format!(" {{{}}}", prop_pairs.join(", "))
            }
        } else {
            String::new()
        };

        let query_str = format!(
            "CREATE (n{}{}) RETURN id(n), labels(n), properties(n)",
            label_str, props_str
        );

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        if result.rows.is_empty() {
            return Err("Failed to create node".into());
        }

        let row = &result.rows[0];
        if row.values.len() >= 3 {
            let id: u64 = serde_json::from_value(row.values[0].clone())
                .map_err(|_| "Failed to parse node ID")?;
            let labels: Vec<String> =
                serde_json::from_value(row.values[1].clone()).unwrap_or_default();
            let props: std::collections::HashMap<String, PropertyValue> =
                serde_json::from_value(row.values[2].clone()).unwrap_or_default();

            Ok(Node {
                id: ID::from(id.to_string()),
                labels,
                properties: props,
            })
        } else {
            Err("Invalid response from database".into())
        }
    }

    /// Update a node's properties
    async fn update_node(
        &self,
        ctx: &Context<'_>,
        id: String,
        properties: std::collections::HashMap<String, PropertyValue>,
    ) -> GQLResult<Node> {
        let server = ctx.data::<Arc<NexusServer>>()?;

        let node_id: u64 = id.parse().map_err(|_| "Invalid node ID format")?;

        if properties.is_empty() {
            return Err("No properties to update".into());
        }

        // Build SET query
        let set_clauses: Vec<String> = properties
            .iter()
            .map(|(k, v)| format!("n.{} = {}", k, property_value_to_cypher(v)))
            .collect();

        let query_str = format!(
            "MATCH (n) WHERE id(n) = {} SET {} RETURN id(n), labels(n), properties(n)",
            node_id,
            set_clauses.join(", ")
        );

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        if result.rows.is_empty() {
            return Err("Node not found".into());
        }

        let row = &result.rows[0];
        if row.values.len() >= 3 {
            let id: u64 = serde_json::from_value(row.values[0].clone())
                .map_err(|_| "Failed to parse node ID")?;
            let labels: Vec<String> =
                serde_json::from_value(row.values[1].clone()).unwrap_or_default();
            let props: std::collections::HashMap<String, PropertyValue> =
                serde_json::from_value(row.values[2].clone()).unwrap_or_default();

            Ok(Node {
                id: ID::from(id.to_string()),
                labels,
                properties: props,
            })
        } else {
            Err("Invalid response from database".into())
        }
    }

    /// Delete a node
    async fn delete_node(
        &self,
        ctx: &Context<'_>,
        id: String,
        detach: Option<bool>, // If true, also delete relationships
    ) -> GQLResult<bool> {
        let server = ctx.data::<Arc<NexusServer>>()?;

        let node_id: u64 = id.parse().map_err(|_| "Invalid node ID format")?;

        let query_str = if detach.unwrap_or(false) {
            format!("MATCH (n) WHERE id(n) = {} DETACH DELETE n", node_id)
        } else {
            format!("MATCH (n) WHERE id(n) = {} DELETE n", node_id)
        };

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        server.executor.execute(&query)?;
        Ok(true)
    }

    /// Create a new relationship between two nodes
    async fn create_relationship(
        &self,
        ctx: &Context<'_>,
        from_id: String,
        to_id: String,
        rel_type: String,
        properties: Option<std::collections::HashMap<String, PropertyValue>>,
    ) -> GQLResult<Relationship> {
        let server = ctx.data::<Arc<NexusServer>>()?;

        let from_node_id: u64 = from_id.parse().map_err(|_| "Invalid from node ID format")?;
        let to_node_id: u64 = to_id.parse().map_err(|_| "Invalid to node ID format")?;

        // Build CREATE query
        let props_str = if let Some(props) = &properties {
            if props.is_empty() {
                String::new()
            } else {
                let prop_pairs: Vec<String> = props
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, property_value_to_cypher(v)))
                    .collect();
                format!(" {{{}}}", prop_pairs.join(", "))
            }
        } else {
            String::new()
        };

        let query_str = format!(
            "MATCH (a), (b) WHERE id(a) = {} AND id(b) = {} CREATE (a)-[r:{}{}]->(b) RETURN id(r), '{}', id(a), id(b), properties(r)",
            from_node_id, to_node_id, rel_type, props_str, rel_type
        );

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        if result.rows.is_empty() {
            return Err("Failed to create relationship (nodes may not exist)".into());
        }

        let row = &result.rows[0];
        if row.values.len() >= 5 {
            let rel_id: u64 = serde_json::from_value(row.values[0].clone())
                .map_err(|_| "Failed to parse relationship ID")?;
            let rel_type: String =
                serde_json::from_value(row.values[1].clone()).unwrap_or_default();
            let from: u64 = serde_json::from_value(row.values[2].clone()).unwrap_or(0);
            let to: u64 = serde_json::from_value(row.values[3].clone()).unwrap_or(0);
            let props: std::collections::HashMap<String, PropertyValue> =
                serde_json::from_value(row.values[4].clone()).unwrap_or_default();

            Ok(Relationship {
                id: ID::from(rel_id.to_string()),
                rel_type,
                from: ID::from(from.to_string()),
                to: ID::from(to.to_string()),
                properties: props,
            })
        } else {
            Err("Invalid response from database".into())
        }
    }

    /// Delete a relationship
    async fn delete_relationship(&self, ctx: &Context<'_>, id: String) -> GQLResult<bool> {
        let server = ctx.data::<Arc<NexusServer>>()?;

        let rel_id: u64 = id.parse().map_err(|_| "Invalid relationship ID format")?;

        let query_str = format!("MATCH ()-[r]->() WHERE id(r) = {} DELETE r", rel_id);

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        server.executor.execute(&query)?;
        Ok(true)
    }

    /// Execute a raw Cypher mutation query
    async fn execute_cypher(
        &self,
        ctx: &Context<'_>,
        query_str: String,
    ) -> GQLResult<MutationResult> {
        let server = ctx.data::<Arc<NexusServer>>()?;

        let start = std::time::Instant::now();

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        let elapsed = start.elapsed().as_millis() as i64;

        Ok(MutationResult {
            success: true,
            message: format!("Query executed successfully in {}ms", elapsed),
            affected_nodes: result.rows.len() as i64,
            execution_time_ms: elapsed,
        })
    }
}

/// Result of a mutation operation
#[derive(Debug, Clone)]
pub struct MutationResult {
    pub success: bool,
    pub message: String,
    pub affected_nodes: i64,
    pub execution_time_ms: i64,
}

#[Object]
impl MutationResult {
    async fn success(&self) -> bool {
        self.success
    }

    async fn message(&self) -> &str {
        &self.message
    }

    async fn affected_nodes(&self) -> i64 {
        self.affected_nodes
    }

    async fn execution_time_ms(&self) -> i64 {
        self.execution_time_ms
    }
}

// Helper function to convert PropertyValue to Cypher literal
fn property_value_to_cypher(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Null => "null".to_string(),
        PropertyValue::Boolean(b) => b.to_string(),
        PropertyValue::Integer(i) => i.to_string(),
        PropertyValue::Float(f) => f.to_string(),
        PropertyValue::String(s) => format!("'{}'", s.replace('\'', "\\'")),
        PropertyValue::List(list) => {
            let items: Vec<String> = list.iter().map(property_value_to_cypher).collect();
            format!("[{}]", items.join(", "))
        }
        PropertyValue::Map(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", k, property_value_to_cypher(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
    }
}
