//! GraphQL query resolvers
//!
//! This module implements field-level resolvers for nodes and relationships,
//! providing efficient data fetching and relationship traversal.

use crate::NexusServer;
use crate::api::graphql::types::*;
use async_graphql::{Context, Object, Result as GQLResult};
use nexus_core::executor::Query;
use std::collections::HashMap;
use std::sync::Arc;

#[Object]
impl Node {
    /// Get the node ID
    async fn id(&self) -> &str {
        &self.id
    }

    /// Get node labels
    async fn labels(&self) -> &[String] {
        &self.labels
    }

    /// Get all properties
    async fn properties(&self) -> &std::collections::HashMap<String, PropertyValue> {
        &self.properties
    }

    /// Get a specific property by key
    async fn property(&self, key: String) -> Option<&PropertyValue> {
        self.properties.get(&key)
    }

    /// Get outgoing relationships
    async fn outgoing_relationships(
        &self,
        ctx: &Context<'_>,
        rel_type: Option<String>,
    ) -> GQLResult<Vec<Relationship>> {
        let server = ctx.data::<Arc<NexusServer>>()?;
        let node_id = self.id.parse::<u64>().map_err(|_| "Invalid node ID")?;

        let query_str = if let Some(rt) = rel_type {
            format!(
                "MATCH (n)-[r:{}]->(m) WHERE id(n) = {} RETURN id(r), '{}', id(n), id(m), properties(r)",
                rt, node_id, rt
            )
        } else {
            format!(
                "MATCH (n)-[r]->(m) WHERE id(n) = {} RETURN id(r), type(r), id(n), id(m), properties(r)",
                node_id
            )
        };

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        let mut relationships = Vec::new();
        for row in result.rows {
            if row.values.len() >= 5 {
                use async_graphql::ID;

                let rel_id: u64 = serde_json::from_value(row.values[0].clone()).unwrap_or(0);
                let rel_type: String =
                    serde_json::from_value(row.values[1].clone()).unwrap_or_default();
                let from: u64 = serde_json::from_value(row.values[2].clone()).unwrap_or(0);
                let to: u64 = serde_json::from_value(row.values[3].clone()).unwrap_or(0);
                let props: HashMap<String, PropertyValue> =
                    serde_json::from_value(row.values[4].clone()).unwrap_or_default();

                relationships.push(Relationship {
                    id: ID::from(rel_id.to_string()),
                    rel_type,
                    from: ID::from(from.to_string()),
                    to: ID::from(to.to_string()),
                    properties: props,
                });
            }
        }

        Ok(relationships)
    }

    /// Get incoming relationships
    async fn incoming_relationships(
        &self,
        ctx: &Context<'_>,
        rel_type: Option<String>,
    ) -> GQLResult<Vec<Relationship>> {
        let server = ctx.data::<Arc<NexusServer>>()?;
        let node_id = self.id.parse::<u64>().map_err(|_| "Invalid node ID")?;

        let query_str = if let Some(rt) = rel_type {
            format!(
                "MATCH (m)-[r:{}]->(n) WHERE id(n) = {} RETURN id(r), '{}', id(m), id(n), properties(r)",
                rt, node_id, rt
            )
        } else {
            format!(
                "MATCH (m)-[r]->(n) WHERE id(n) = {} RETURN id(r), type(r), id(m), id(n), properties(r)",
                node_id
            )
        };

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        let mut relationships = Vec::new();
        for row in result.rows {
            if row.values.len() >= 5 {
                use async_graphql::ID;

                let rel_id: u64 = serde_json::from_value(row.values[0].clone()).unwrap_or(0);
                let rel_type: String =
                    serde_json::from_value(row.values[1].clone()).unwrap_or_default();
                let from: u64 = serde_json::from_value(row.values[2].clone()).unwrap_or(0);
                let to: u64 = serde_json::from_value(row.values[3].clone()).unwrap_or(0);
                let props: HashMap<String, PropertyValue> =
                    serde_json::from_value(row.values[4].clone()).unwrap_or_default();

                relationships.push(Relationship {
                    id: ID::from(rel_id.to_string()),
                    rel_type,
                    from: ID::from(from.to_string()),
                    to: ID::from(to.to_string()),
                    properties: props,
                });
            }
        }

        Ok(relationships)
    }

    /// Get all relationships (both incoming and outgoing)
    async fn all_relationships(
        &self,
        ctx: &Context<'_>,
        rel_type: Option<String>,
    ) -> GQLResult<Vec<Relationship>> {
        let server = ctx.data::<Arc<NexusServer>>()?;
        let node_id = self.id.parse::<u64>().map_err(|_| "Invalid node ID")?;

        // Fetch outgoing relationships
        let outgoing_query = if let Some(ref rt) = rel_type {
            format!(
                "MATCH (n)-[r:{}]->(m) WHERE id(n) = {} RETURN id(r), '{}', id(n), id(m), properties(r)",
                rt, node_id, rt
            )
        } else {
            format!(
                "MATCH (n)-[r]->(m) WHERE id(n) = {} RETURN id(r), type(r), id(n), id(m), properties(r)",
                node_id
            )
        };

        // Fetch incoming relationships
        let incoming_query = if let Some(ref rt) = rel_type {
            format!(
                "MATCH (m)-[r:{}]->(n) WHERE id(n) = {} RETURN id(r), '{}', id(m), id(n), properties(r)",
                rt, node_id, rt
            )
        } else {
            format!(
                "MATCH (m)-[r]->(n) WHERE id(n) = {} RETURN id(r), type(r), id(m), id(n), properties(r)",
                node_id
            )
        };

        let mut relationships = Vec::new();

        // Execute outgoing query
        let query = Query {
            cypher: outgoing_query,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        for row in result.rows {
            if row.values.len() >= 5 {
                use async_graphql::ID;

                let rel_id: u64 = serde_json::from_value(row.values[0].clone()).unwrap_or(0);
                let rel_type_val: String =
                    serde_json::from_value(row.values[1].clone()).unwrap_or_default();
                let from: u64 = serde_json::from_value(row.values[2].clone()).unwrap_or(0);
                let to: u64 = serde_json::from_value(row.values[3].clone()).unwrap_or(0);
                let props: HashMap<String, PropertyValue> =
                    serde_json::from_value(row.values[4].clone()).unwrap_or_default();

                relationships.push(Relationship {
                    id: ID::from(rel_id.to_string()),
                    rel_type: rel_type_val,
                    from: ID::from(from.to_string()),
                    to: ID::from(to.to_string()),
                    properties: props,
                });
            }
        }

        // Execute incoming query
        let query = Query {
            cypher: incoming_query,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        for row in result.rows {
            if row.values.len() >= 5 {
                use async_graphql::ID;

                let rel_id: u64 = serde_json::from_value(row.values[0].clone()).unwrap_or(0);
                let rel_type_val: String =
                    serde_json::from_value(row.values[1].clone()).unwrap_or_default();
                let from: u64 = serde_json::from_value(row.values[2].clone()).unwrap_or(0);
                let to: u64 = serde_json::from_value(row.values[3].clone()).unwrap_or(0);
                let props: HashMap<String, PropertyValue> =
                    serde_json::from_value(row.values[4].clone()).unwrap_or_default();

                relationships.push(Relationship {
                    id: ID::from(rel_id.to_string()),
                    rel_type: rel_type_val,
                    from: ID::from(from.to_string()),
                    to: ID::from(to.to_string()),
                    properties: props,
                });
            }
        }

        Ok(relationships)
    }

    /// Get related nodes through outgoing relationships
    async fn related_nodes(
        &self,
        ctx: &Context<'_>,
        rel_type: Option<String>,
        direction: Option<String>, // "OUT", "IN", "BOTH"
    ) -> GQLResult<Vec<Node>> {
        let server = ctx.data::<Arc<NexusServer>>()?;
        let node_id = self.id.parse::<u64>().map_err(|_| "Invalid node ID")?;

        let direction = direction.unwrap_or_else(|| "OUT".to_string());

        let query_str = match direction.to_uppercase().as_str() {
            "OUT" => {
                if let Some(rt) = rel_type {
                    format!(
                        "MATCH (n)-[:{}]->(m) WHERE id(n) = {} RETURN id(m), labels(m), properties(m)",
                        rt, node_id
                    )
                } else {
                    format!(
                        "MATCH (n)-[]->(m) WHERE id(n) = {} RETURN id(m), labels(m), properties(m)",
                        node_id
                    )
                }
            }
            "IN" => {
                if let Some(rt) = rel_type {
                    format!(
                        "MATCH (m)-[:{}]->(n) WHERE id(n) = {} RETURN id(m), labels(m), properties(m)",
                        rt, node_id
                    )
                } else {
                    format!(
                        "MATCH (m)-[]->(n) WHERE id(n) = {} RETURN id(m), labels(m), properties(m)",
                        node_id
                    )
                }
            }
            _ => {
                if let Some(rt) = rel_type {
                    format!(
                        "MATCH (n)-[:{}]-(m) WHERE id(n) = {} RETURN id(m), labels(m), properties(m)",
                        rt, node_id
                    )
                } else {
                    format!(
                        "MATCH (n)-[]-(m) WHERE id(n) = {} RETURN id(m), labels(m), properties(m)",
                        node_id
                    )
                }
            }
        };

        // Execute query
        let query = Query {
            cypher: query_str,
            params: HashMap::new(),
        };
        let result = server.executor.execute(&query)?;

        let mut nodes = Vec::new();
        for row in result.rows {
            if row.values.len() >= 3 {
                use async_graphql::ID;

                let id: u64 = serde_json::from_value(row.values[0].clone()).unwrap_or(0);
                let labels: Vec<String> =
                    serde_json::from_value(row.values[1].clone()).unwrap_or_default();
                let properties: HashMap<String, PropertyValue> =
                    serde_json::from_value(row.values[2].clone()).unwrap_or_default();

                nodes.push(Node {
                    id: ID::from(id.to_string()),
                    labels,
                    properties,
                });
            }
        }

        Ok(nodes)
    }
}

#[Object]
impl Relationship {
    async fn id(&self) -> &str {
        &self.id
    }

    async fn rel_type(&self) -> &str {
        &self.rel_type
    }

    async fn from(&self) -> &str {
        &self.from
    }

    async fn to(&self) -> &str {
        &self.to
    }

    async fn properties(&self) -> &std::collections::HashMap<String, PropertyValue> {
        &self.properties
    }

    async fn property(&self, key: String) -> Option<&PropertyValue> {
        self.properties.get(&key)
    }

    /// Get the source node
    async fn from_node(&self, ctx: &Context<'_>) -> GQLResult<Option<Node>> {
        let server = ctx.data::<Arc<NexusServer>>()?;
        let node_id = self.from.parse::<u64>().map_err(|_| "Invalid node ID")?;

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

        let row = &result.rows[0];
        if row.values.len() >= 3 {
            use async_graphql::ID;

            let id: u64 = serde_json::from_value(row.values[0].clone()).unwrap_or(0);
            let labels: Vec<String> =
                serde_json::from_value(row.values[1].clone()).unwrap_or_default();
            let properties: HashMap<String, PropertyValue> =
                serde_json::from_value(row.values[2].clone()).unwrap_or_default();

            Ok(Some(Node {
                id: ID::from(id.to_string()),
                labels,
                properties,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get the target node
    async fn to_node(&self, ctx: &Context<'_>) -> GQLResult<Option<Node>> {
        let server = ctx.data::<Arc<NexusServer>>()?;
        let node_id = self.to.parse::<u64>().map_err(|_| "Invalid node ID")?;

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

        let row = &result.rows[0];
        if row.values.len() >= 3 {
            use async_graphql::ID;

            let id: u64 = serde_json::from_value(row.values[0].clone()).unwrap_or(0);
            let labels: Vec<String> =
                serde_json::from_value(row.values[1].clone()).unwrap_or_default();
            let properties: HashMap<String, PropertyValue> =
                serde_json::from_value(row.values[2].clone()).unwrap_or_default();

            Ok(Some(Node {
                id: ID::from(id.to_string()),
                labels,
                properties,
            }))
        } else {
            Ok(None)
        }
    }
}
