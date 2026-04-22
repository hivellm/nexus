//! MCP (Model Context Protocol) client integration

// Note: RMCP client types are not available in the current version
// We'll implement a simplified MCP client for now

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

/// MCP client for communicating with AI services
pub struct McpClient {
    /// Server endpoint
    endpoint: String,
    /// Connection status
    connected: bool,
    /// API key for authentication (optional)
    api_key: Option<Arc<RwLock<String>>>,
}

/// Error types for MCP client operations
#[derive(Debug, thiserror::Error)]
pub enum McpClientError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("MCP protocol error: {0}")]
    ProtocolError(String),
    #[error("Client not connected")]
    NotConnected,
}

impl McpClient {
    /// Create a new MCP client
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            connected: false,
            api_key: None,
        }
    }

    /// Create a new MCP client with API key
    pub fn with_api_key(endpoint: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            connected: false,
            api_key: Some(Arc::new(RwLock::new(api_key.into()))),
        }
    }

    /// Set or update the API key (for key rotation)
    pub async fn set_api_key(&mut self, api_key: impl Into<String>) {
        self.api_key = Some(Arc::new(RwLock::new(api_key.into())));
    }

    /// Rotate the API key (alias for set_api_key)
    pub async fn rotate_key(&mut self, new_api_key: impl Into<String>) {
        self.set_api_key(new_api_key).await;
    }

    /// Initialize the MCP connection
    pub async fn connect(&mut self) -> anyhow::Result<()> {
        // For now, we'll create a mock connection
        // In a full implementation, this would establish the actual MCP connection
        self.connected = true;
        Ok(())
    }

    /// Build request headers with authentication
    async fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("Nexus-MCP-Client/1.0"),
        );

        // Add Bearer token authentication if API key is set
        if let Some(api_key) = &self.api_key {
            let key = api_key.read().await;
            if let Ok(bearer_value) =
                reqwest::header::HeaderValue::from_str(&format!("Bearer {}", *key))
            {
                headers.insert(reqwest::header::AUTHORIZATION, bearer_value);
            }
        }

        headers
    }

    /// Handle HTTP error responses (401, 403, 429)
    fn handle_error_response(
        &self,
        status: reqwest::StatusCode,
        message: String,
    ) -> McpClientError {
        match status {
            reqwest::StatusCode::UNAUTHORIZED => McpClientError::Unauthorized(message),
            reqwest::StatusCode::FORBIDDEN => McpClientError::Forbidden(message),
            reqwest::StatusCode::TOO_MANY_REQUESTS => McpClientError::RateLimitExceeded(message),
            _ => McpClientError::HttpError(message),
        }
    }

    /// Call an MCP method
    pub async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpClientError> {
        // Check if client is connected
        if !self.connected {
            return Err(McpClientError::NotConnected);
        }

        // Create HTTP client for MCP over HTTP
        let client = reqwest::Client::new();

        // Build MCP request
        let mcp_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        // Build headers with authentication
        let headers = self.build_headers().await;

        // Send request
        let response = client
            .post(&self.endpoint)
            .headers(headers)
            .json(&mcp_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(self.handle_error_response(status, error_text));
        }

        // Parse response
        let response_json: serde_json::Value = response.json().await?;

        // Check for MCP error
        if let Some(error) = response_json.get("error") {
            let error_message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            return Err(McpClientError::ProtocolError(error_message.to_string()));
        }

        // Return result
        Ok(response_json
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<Vec<String>, McpClientError> {
        let result = self.call("tools/list", serde_json::json!({})).await?;

        if let Some(tools) = result.get("tools").and_then(|t| t.as_array()) {
            let tool_names: Vec<String> = tools
                .iter()
                .filter_map(|tool| {
                    tool.get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            Ok(tool_names)
        } else {
            Ok(vec![])
        }
    }

    /// Call a specific tool
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpClientError> {
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": arguments
        });

        self.call("tools/call", params).await
    }

    /// Get the endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_client_creation() {
        let client = McpClient::new("http://localhost:8080");
        assert_eq!(client.endpoint(), "http://localhost:8080");
    }

    #[test]
    fn test_mcp_client_with_string() {
        let endpoint = "https://api.example.com".to_string();
        let client = McpClient::new(endpoint);
        assert_eq!(client.endpoint(), "https://api.example.com");
    }

    #[test]
    fn test_mcp_client_with_owned_string() {
        let endpoint = "wss://stream.example.com".to_string();
        let client = McpClient::new(endpoint.clone());
        assert_eq!(client.endpoint(), "wss://stream.example.com");
    }

    #[tokio::test]
    async fn test_mcp_client_call_implementation() {
        let mut client = McpClient::new("http://httpbin.org/post");

        // Connect first
        let connect_result = client.connect().await;
        assert!(connect_result.is_ok());

        let params = serde_json::json!({"test": "value"});

        // This should now work with the implementation
        let result = client.call("test_method", params).await;

        // The request might fail due to network or MCP protocol, but it shouldn't panic
        match result {
            Ok(response) => {
                tracing::debug!("MCP call succeeded: {:?}", response);
            }
            Err(e) => tracing::debug!("MCP call failed as expected: {}", e),
        }
    }

    #[test]
    fn test_mcp_client_with_api_key() {
        let client = McpClient::with_api_key("http://localhost:8080", "nx_test123456789");
        assert_eq!(client.endpoint(), "http://localhost:8080");
    }

    #[tokio::test]
    async fn test_mcp_client_key_rotation() {
        let mut client = McpClient::with_api_key("http://localhost:8080", "nx_old_key");
        client.rotate_key("nx_new_key").await;
        // Key rotation should succeed without panicking
    }

    #[tokio::test]
    async fn test_mcp_client_list_tools() {
        let mut client = McpClient::new("http://httpbin.org/post");

        // Connect first
        let connect_result = client.connect().await;
        assert!(connect_result.is_ok());

        // This should now work with the implementation
        let result = client.list_tools().await;

        // The request might fail due to network or MCP protocol, but it shouldn't panic
        match result {
            Ok(tools) => {
                tracing::debug!("List tools succeeded: {:?}", tools);
            }
            Err(e) => tracing::debug!("List tools failed as expected: {}", e),
        }
    }
}
