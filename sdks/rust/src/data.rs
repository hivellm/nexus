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
pub struct CreateRelationshipRequest {
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
pub struct CreateRelationshipResponse {
    /// Created relationship ID
    pub rel_id: u64,
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
    /// # use nexus_sdk_rust::NexusClient;
    /// # use std::collections::HashMap;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let mut properties = HashMap::new();
    /// properties.insert("name".to_string(), nexus_sdk_rust::Value::String("Alice".to_string()));
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
    /// * `node_id` - Node ID to retrieve
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.get_node(1).await?;
    /// if let Some(node) = response.node {
    ///     println!("Node ID: {}, Labels: {:?}", node.id, node.labels);
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

    /// Update a node
    ///
    /// # Arguments
    ///
    /// * `node_id` - Node ID to update
    /// * `properties` - New properties (will replace existing)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::NexusClient;
    /// # use std::collections::HashMap;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let mut properties = HashMap::new();
    /// properties.insert("age".to_string(), nexus_sdk_rust::Value::Int(30));
    /// let response = client.update_node(1, properties).await?;
    /// println!("Update result: {}", response.message);
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

    /// Delete a node
    ///
    /// # Arguments
    ///
    /// * `node_id` - Node ID to delete
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.delete_node(1).await?;
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
    /// * `source_id` - Source node ID
    /// * `target_id` - Target node ID
    /// * `rel_type` - Relationship type
    /// * `properties` - Relationship properties
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk_rust::NexusClient;
    /// # use std::collections::HashMap;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk_rust::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let properties = HashMap::new();
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
    ) -> Result<CreateRelationshipResponse> {
        let request = CreateRelationshipRequest {
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
            let result: CreateRelationshipResponse = response.json().await?;
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
}
