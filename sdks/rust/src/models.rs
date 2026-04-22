//! Data models for Nexus SDK

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Cypher query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Column names
    pub columns: Vec<String>,
    /// Result rows (each row is an array of values)
    pub rows: Vec<serde_json::Value>,
    /// Execution time in milliseconds
    #[serde(rename = "execution_time_ms")]
    pub execution_time_ms: Option<u64>,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A single row in a query result (helper for accessing row values)
#[derive(Debug, Clone)]
pub struct Row {
    /// Row values
    pub values: Vec<Value>,
}

impl Row {
    /// Convert from serde_json::Value (array) to Row
    pub fn from_json_value(value: &serde_json::Value) -> Option<Self> {
        if let serde_json::Value::Array(arr) = value {
            let values: Result<Vec<Value>, _> = arr
                .iter()
                .map(|v| serde_json::from_value(v.clone()))
                .collect();
            values.ok().map(|values| Row { values })
        } else {
            None
        }
    }
}

/// Value types in query results
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value
    Int(i64),
    /// Float value
    Float(f64),
    /// String value
    String(String),
    /// Array value
    Array(Vec<Value>),
    /// Object value
    Object(HashMap<String, Value>),
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Value::Int(i as i64)
    }
}

impl From<usize> for Value {
    fn from(u: usize) -> Self {
        Value::Int(u as i64)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<f32> for Value {
    fn from(f: f32) -> Self {
        Value::Float(f as f64)
    }
}

/// Node representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Node ID
    pub id: u64,
    /// Node labels
    pub labels: Vec<String>,
    /// Node properties
    pub properties: HashMap<String, Value>,
}

/// Relationship representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    /// Relationship ID
    pub id: u64,
    /// Relationship type
    #[serde(rename = "type")]
    pub rel_type: String,
    /// Source node ID
    pub source_id: u64,
    /// Target node ID
    pub target_id: u64,
    /// Relationship properties
    pub properties: HashMap<String, Value>,
}

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    /// Catalog statistics
    pub catalog: CatalogStats,
    /// Label index statistics
    #[serde(rename = "label_index")]
    pub label_index: LabelIndexStats,
    /// KNN index statistics
    #[serde(rename = "knn_index")]
    pub knn_index: KnnIndexStats,
}

/// Catalog statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogStats {
    /// Number of labels
    #[serde(rename = "label_count")]
    pub label_count: usize,
    /// Number of relationship types
    #[serde(rename = "rel_type_count")]
    pub rel_type_count: usize,
    /// Number of nodes
    #[serde(rename = "node_count")]
    pub node_count: usize,
    /// Number of relationships
    #[serde(rename = "rel_count")]
    pub rel_count: usize,
}

/// Label index statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabelIndexStats {
    /// Number of indexed labels
    #[serde(rename = "indexed_labels")]
    pub indexed_labels: usize,
    /// Total number of nodes
    #[serde(rename = "total_nodes")]
    pub total_nodes: usize,
}

/// KNN index statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnnIndexStats {
    /// Total number of vectors
    #[serde(rename = "total_vectors")]
    pub total_vectors: usize,
    /// Vector dimension
    pub dimension: usize,
    /// Average search time in microseconds
    #[serde(rename = "avg_search_time_us")]
    pub avg_search_time_us: f64,
}

/// Client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Endpoint URL. Accepts `nexus://host[:port]` (RPC default),
    /// `http://host[:port]`, `https://host[:port]`, or bare
    /// `host[:port]` (treated as `nexus://`).
    pub base_url: String,
    /// Explicit transport override. `None` means "infer from URL
    /// scheme or NEXUS_SDK_TRANSPORT env var". See
    /// [`docs/specs/sdk-transport.md`] for the precedence rules.
    pub transport: Option<crate::transport::TransportMode>,
    /// Default RPC port when the URL does not carry one.
    pub rpc_port: u16,
    /// Default RESP3 port when the URL does not carry one. Reserved;
    /// not yet used by the SDK but accepted so a config that targets
    /// a non-default RESP3 port compiles cleanly.
    pub resp3_port: u16,
    /// API key for authentication (optional)
    pub api_key: Option<String>,
    /// Username for authentication (optional)
    pub username: Option<String>,
    /// Password for authentication (optional)
    pub password: Option<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Maximum number of retries
    pub max_retries: u32,
}

impl Default for ClientConfig {
    /// Default endpoint points at the native Nexus RPC listener on
    /// the loopback. New SDK users get the fastest transport without
    /// code changes; callers who need HTTP pass `http://host:15474`
    /// or set `transport = Some(TransportMode::Http)`.
    fn default() -> Self {
        Self {
            base_url: "nexus://127.0.0.1:15475".to_string(),
            transport: None,
            rpc_port: 15475,
            resp3_port: 15476,
            api_key: None,
            username: None,
            password: None,
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

/// Cypher query request
#[derive(Debug, Clone, Serialize)]
pub struct CypherRequest {
    /// Cypher query string
    pub query: String,
    /// Query parameters (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<HashMap<String, Value>>,
}

// ============================================================================
// Database Management Models
// ============================================================================

/// Database information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfo {
    /// Database name
    pub name: String,
    /// Database path
    pub path: String,
    /// Creation timestamp
    #[serde(rename = "created_at")]
    pub created_at: i64,
    /// Number of nodes
    #[serde(rename = "node_count")]
    pub node_count: u64,
    /// Number of relationships
    #[serde(rename = "relationship_count")]
    pub relationship_count: u64,
    /// Storage size in bytes
    #[serde(rename = "storage_size")]
    pub storage_size: u64,
}

/// Response for listing databases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDatabasesResponse {
    /// List of databases
    pub databases: Vec<DatabaseInfo>,
    /// Default database name
    #[serde(rename = "default_database")]
    pub default_database: String,
}

/// Request to create a database
#[derive(Debug, Clone, Serialize)]
pub struct CreateDatabaseRequest {
    /// Database name
    pub name: String,
}

/// Response for creating a database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDatabaseResponse {
    /// Success flag
    pub success: bool,
    /// Database name
    pub name: String,
    /// Message
    pub message: String,
}

/// Response for dropping a database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropDatabaseResponse {
    /// Success flag
    pub success: bool,
    /// Message
    pub message: String,
}

/// Response for session database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDatabaseResponse {
    /// Current database name
    pub database: String,
}

/// Request to switch database
#[derive(Debug, Clone, Serialize)]
pub struct SwitchDatabaseRequest {
    /// Database name to switch to
    pub name: String,
}

/// Response for switching database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchDatabaseResponse {
    /// Success flag
    pub success: bool,
    /// Message
    pub message: String,
}
