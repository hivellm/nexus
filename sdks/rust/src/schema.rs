//! Schema management operations

use crate::client::NexusClient;
use crate::error::{NexusError, Result};
use serde::{Deserialize, Serialize};

/// Create label request
#[derive(Debug, Clone, Serialize)]
pub struct CreateLabelRequest {
    /// Label name
    pub name: String,
}

/// Create label response
#[derive(Debug, Clone, Deserialize)]
pub struct CreateLabelResponse {
    /// Label ID (may be 0 if error)
    #[serde(default)]
    pub label_id: u32,
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// List labels response
#[derive(Debug, Clone, Deserialize)]
pub struct ListLabelsResponse {
    /// List of labels (as tuples of name and ID)
    pub labels: Vec<(String, u32)>,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Create relationship type request
#[derive(Debug, Clone, Serialize)]
pub struct CreateRelTypeRequest {
    /// Relationship type name
    pub name: String,
}

/// Create relationship type response
#[derive(Debug, Clone, Deserialize)]
pub struct CreateRelTypeResponse {
    /// Relationship type ID (may be 0 if error)
    #[serde(default)]
    pub type_id: u32,
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// List relationship types response
#[derive(Debug, Clone, Deserialize)]
pub struct ListRelTypesResponse {
    /// List of relationship types (as tuples of name and ID)
    pub types: Vec<(String, u32)>,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl NexusClient {
    /// Create a new label
    ///
    /// # Arguments
    ///
    /// * `name` - Label name
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.create_label("Person".to_string()).await?;
    /// tracing::info!("Create label result: {}", response.message);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_label(&self, name: String) -> Result<CreateLabelResponse> {
        let request = CreateLabelRequest { name };

        let url = self.get_base_url().join("/schema/labels")?;
        let mut request_builder = self.get_client().post(url).json(&request);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: CreateLabelResponse = response.json().await?;
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

    /// List all labels
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.list_labels().await?;
    /// tracing::info!("Labels: {:?}", response.labels);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_labels(&self) -> Result<ListLabelsResponse> {
        let url = self.get_base_url().join("/schema/labels")?;
        let mut request_builder = self.get_client().get(url);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: ListLabelsResponse = response.json().await?;
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

    /// Create a new relationship type
    ///
    /// # Arguments
    ///
    /// * `name` - Relationship type name
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.create_rel_type("KNOWS".to_string()).await?;
    /// tracing::info!("Create rel type result: {}", response.message);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_rel_type(&self, name: String) -> Result<CreateRelTypeResponse> {
        let request = CreateRelTypeRequest { name };

        let url = self.get_base_url().join("/schema/rel_types")?;
        let mut request_builder = self.get_client().post(url).json(&request);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: CreateRelTypeResponse = response.json().await?;
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

    /// List all relationship types
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.list_rel_types().await?;
    /// tracing::info!("Relationship types: {:?}", response.types);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_rel_types(&self) -> Result<ListRelTypesResponse> {
        let url = self.get_base_url().join("/schema/rel_types")?;
        let mut request_builder = self.get_client().get(url);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: ListRelTypesResponse = response.json().await?;
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
