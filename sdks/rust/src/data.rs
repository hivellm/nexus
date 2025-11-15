//! Data operations (nodes and relationships)

use crate::client::NexusClient;
use crate::error::{NexusError, Result};
use crate::models::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Create node request
#[derive(Debug, Clone, Serialize)]
pub struct CreateNodeRequest {
    /// Node labels
    pub labels: Vec<String>,
    /// Node properties
    #[serde(default)]
    pub properties: HashMap<String, Value>,
}

/// Create node response
#[derive(Debug, Clone, Deserialize)]
pub struct CreateNodeResponse {
    /// Created node ID
    pub node_id: u64,
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Get node response
#[derive(Debug, Clone, Deserialize)]
pub struct GetNodeResponse {
    /// Node data
    pub node: Option<Node>,
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Update node request
#[derive(Debug, Clone, Serialize)]
pub struct UpdateNodeRequest {
    /// Node ID
    pub node_id: u64,
    /// New properties (will replace existing)
    pub properties: HashMap<String, Value>,
}

/// Update node response
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateNodeResponse {
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Delete node response
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteNodeResponse {
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Create relationship request
#[derive(Debug, Clone, Serialize)]
pub struct CreateRelRequest {
    /// Source node ID
    pub source_id: u64,
    /// Target node ID
    pub target_id: u64,
    /// Relationship type
    pub rel_type: String,
    /// Relationship properties
    #[serde(default)]
    pub properties: HashMap<String, Value>,
}

/// Create relationship response
#[derive(Debug, Clone, Deserialize)]
pub struct CreateRelResponse {
    /// Relationship ID
    pub rel_id: u64,
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Update relationship request
#[derive(Debug, Clone, Serialize)]
pub struct UpdateRelRequest {
    /// Relationship ID
    pub rel_id: u64,
    /// New properties (will replace existing)
    pub properties: HashMap<String, Value>,
}

/// Update relationship response
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRelResponse {
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Delete relationship response
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteRelResponse {
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl NexusClient {
    /// Create a new node
    ///
    /// # Arguments
    ///
    /// * `labels` - Node labels
    /// * `properties` - Node properties
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::{NexusClient, Value};
    /// # use std::collections::HashMap;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let mut properties = HashMap::new();
    /// properties.insert("name".to_string(), Value::String("Alice".to_string()));
    /// let response = client.create_node(vec!["Person".to_string()], properties).await?;
    /// println!("Created node with ID: {}", response.node_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_node(
        &self,
        labels: Vec<String>,
        properties: HashMap<String, Value>,
    ) -> Result<CreateNodeResponse> {
        let request = CreateNodeRequest { labels, properties };

        let url = self.get_base_url().join("/data/nodes")?;
        let mut request_builder = self.get_client().post(url).json(&request);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: CreateNodeResponse = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }

    /// Get a node by ID
    ///
    /// # Arguments
    ///
    /// * `node_id` - ID of the node to retrieve
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.get_node(0).await?; // Replace 0 with an actual node ID
    /// if let Some(node) = response.node {
    ///     println!("Retrieved node: {:?}", node);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_node(&self, node_id: u64) -> Result<GetNodeResponse> {
        let url = self
            .get_base_url()
            .join(&format!("/data/nodes?id={}", node_id))?;
        let mut request_builder = self.get_client().get(url);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: GetNodeResponse = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }

    /// Update an existing node
    ///
    /// # Arguments
    ///
    /// * `node_id` - ID of the node to update
    /// * `properties` - New properties for the node
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::{NexusClient, Value};
    /// # use std::collections::HashMap;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let mut properties = HashMap::new();
    /// properties.insert("name".to_string(), Value::String("Bob".to_string()));
    /// let response = client.update_node(0, properties).await?; // Replace 0 with an actual node ID
    /// println!("Update node result: {}", response.message);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_node(
        &self,
        node_id: u64,
        properties: HashMap<String, Value>,
    ) -> Result<UpdateNodeResponse> {
        let request = UpdateNodeRequest {
            node_id,
            properties,
        };

        let url = self.get_base_url().join("/data/nodes")?;
        let mut request_builder = self.get_client().put(url).json(&request);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: UpdateNodeResponse = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }

    /// Delete a node by ID
    ///
    /// # Arguments
    ///
    /// * `node_id` - ID of the node to delete
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.delete_node(0).await?; // Replace 0 with an actual node ID
    /// println!("Delete result: {}", response.message);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_node(&self, node_id: u64) -> Result<DeleteNodeResponse> {
        let url = self
            .get_base_url()
            .join(&format!("/data/nodes?id={}", node_id))?;
        let mut request_builder = self.get_client().delete(url);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: DeleteNodeResponse = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }

    /// Create a new relationship
    ///
    /// # Arguments
    ///
    /// * `source_id` - ID of the source node
    /// * `target_id` - ID of the target node
    /// * `rel_type` - Type of the relationship
    /// * `properties` - Optional relationship properties
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::{NexusClient, Value};
    /// # use std::collections::HashMap;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let mut properties = HashMap::new();
    /// properties.insert("weight".to_string(), Value::Float(1.5));
    /// let response = client.create_relationship(1, 2, "KNOWS".to_string(), properties).await?;
    /// println!("Created relationship with ID: {}", response.rel_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_relationship(
        &self,
        source_id: u64,
        target_id: u64,
        rel_type: String,
        properties: HashMap<String, Value>,
    ) -> Result<CreateRelResponse> {
        let request = CreateRelRequest {
            source_id,
            target_id,
            rel_type,
            properties,
        };

        let url = self.get_base_url().join("/data/relationships")?;
        let mut request_builder = self.get_client().post(url).json(&request);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: CreateRelResponse = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }

    /// Update an existing relationship using Cypher
    ///
    /// # Arguments
    ///
    /// * `rel_id` - ID of the relationship to update
    /// * `properties` - New properties for the relationship
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::{NexusClient, Value};
    /// # use std::collections::HashMap;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let mut properties = HashMap::new();
    /// properties.insert("weight".to_string(), Value::Float(2.0));
    /// let response = client.update_relationship(1, properties).await?;
    /// println!("Update relationship result: {}", response.message);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_relationship(
        &self,
        rel_id: u64,
        properties: HashMap<String, Value>,
    ) -> Result<UpdateRelResponse> {
        // Use Cypher SET to update relationship properties
        let mut props_str = Vec::new();
        let mut params = HashMap::new();

        for (key, value) in properties {
            let param_name = format!("prop_{}", key.replace('-', "_"));
            props_str.push(format!("r.{} = ${}", key, param_name));
            params.insert(param_name, value);
        }

        let query = format!(
            "MATCH ()-[r]->() WHERE id(r) = $rel_id SET {} RETURN r",
            props_str.join(", ")
        );
        params.insert("rel_id".to_string(), Value::Int(rel_id as i64));

        let _result = self.execute_cypher(&query, Some(params)).await?;

        Ok(UpdateRelResponse {
            message: "Relationship updated successfully".to_string(),
            error: None,
        })
    }

    /// Delete a relationship using Cypher
    ///
    /// # Arguments
    ///
    /// * `rel_id` - ID of the relationship to delete
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.delete_relationship(1).await?;
    /// println!("Delete relationship result: {}", response.message);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_relationship(&self, rel_id: u64) -> Result<DeleteRelResponse> {
        let mut params = HashMap::new();
        params.insert("rel_id".to_string(), Value::Int(rel_id as i64));

        let query = "MATCH ()-[r]->() WHERE id(r) = $rel_id DELETE r RETURN count(r) as deleted";
        let _result = self.execute_cypher(query, Some(params)).await?;

        Ok(DeleteRelResponse {
            message: "Relationship deleted successfully".to_string(),
            error: None,
        })
    }
}
