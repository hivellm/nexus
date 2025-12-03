//! GraphQL type definitions

use async_graphql::{ID, InputObject, Object, SimpleObject};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// GraphQL node type representing a graph node
#[derive(Debug, Clone)]
pub struct Node {
    /// Node ID
    pub id: ID,
    /// Node labels
    pub labels: Vec<String>,
    /// Node properties
    pub properties: HashMap<String, PropertyValue>,
}

/// GraphQL relationship type
#[derive(Debug, Clone)]
pub struct Relationship {
    /// Relationship ID
    pub id: ID,
    /// Relationship type
    pub rel_type: String,
    /// Source node ID
    pub from: ID,
    /// Target node ID
    pub to: ID,
    /// Relationship properties
    pub properties: HashMap<String, PropertyValue>,
}

/// Property value type that can hold various types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    List(Vec<PropertyValue>),
    Map(HashMap<String, PropertyValue>),
}

#[Object]
impl PropertyValue {
    async fn as_string(&self) -> Option<String> {
        match self {
            PropertyValue::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    async fn as_int(&self) -> Option<i64> {
        match self {
            PropertyValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    async fn as_float(&self) -> Option<f64> {
        match self {
            PropertyValue::Float(f) => Some(*f),
            PropertyValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    async fn as_bool(&self) -> Option<bool> {
        match self {
            PropertyValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

/// Input type for creating a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNodeInput {
    /// Node labels
    pub labels: Vec<String>,
    /// Node properties
    pub properties: HashMap<String, PropertyValue>,
}

/// Input type for updating a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodeInput {
    /// Node ID
    pub id: ID,
    /// Properties to set/update
    pub properties: HashMap<String, PropertyValue>,
}

/// Input type for creating a relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRelationshipInput {
    /// Relationship type
    #[serde(rename = "type")]
    pub rel_type: String,
    /// Source node ID
    pub from: ID,
    /// Target node ID
    pub to: ID,
    /// Relationship properties
    pub properties: Option<HashMap<String, PropertyValue>>,
}

/// Filter input for querying nodes
#[derive(Debug, Clone, InputObject, Default)]
pub struct NodeFilterInput {
    /// Filter by labels (ANY match)
    pub labels: Option<Vec<String>>,
    /// Filter by property values (not fully supported yet)
    #[graphql(skip)]
    pub properties: Option<HashMap<String, PropertyValue>>,
    /// Limit number of results
    pub limit: Option<i32>,
    /// Skip number of results
    pub skip: Option<i32>,
    /// Order by property
    pub order_by: Option<String>,
    /// Order direction (ASC/DESC)
    pub order_desc: Option<bool>,
}

/// Query result statistics
#[derive(Debug, Clone, SimpleObject)]
pub struct QueryStats {
    /// Execution time in milliseconds
    pub execution_time_ms: i64,
    /// Number of nodes matched
    pub nodes_matched: i64,
    /// Number of relationships matched
    pub relationships_matched: i64,
}
