//! UMICP (Universal Model Interoperability Protocol) client integration

use std::sync::Arc;
use tokio::sync::RwLock;

/// UMICP client for universal model communication
pub struct UmicpClient {
    /// Server endpoint
    endpoint: String,
    /// API key for authentication (optional)
    api_key: Option<Arc<RwLock<String>>>,
}

/// Error types for UMICP client operations
#[derive(Debug, thiserror::Error)]
pub enum UmicpClientError {
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
}

impl UmicpClient {
    /// Create a new UMICP client
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            api_key: None,
        }
    }

    /// Create a new UMICP client with API key
    pub fn with_api_key(endpoint: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
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

    /// Build request headers with authentication
    async fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "X-Protocol",
            reqwest::header::HeaderValue::from_static("UMICP"),
        );
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("Nexus-UMICP-Client/1.0"),
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
    ) -> UmicpClientError {
        match status {
            reqwest::StatusCode::UNAUTHORIZED => UmicpClientError::Unauthorized(message),
            reqwest::StatusCode::FORBIDDEN => UmicpClientError::Forbidden(message),
            reqwest::StatusCode::TOO_MANY_REQUESTS => UmicpClientError::RateLimitExceeded(message),
            _ => UmicpClientError::HttpError(message),
        }
    }

    /// Send a UMICP request
    pub async fn request(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, UmicpClientError> {
        // Create HTTP client
        let client = reqwest::Client::new();

        // Determine the request method based on the payload
        let method = if payload.get("method").is_some() {
            "POST"
        } else {
            "GET"
        };

        // Build headers with authentication
        let headers = self.build_headers().await;

        // Build the request
        let mut request_builder = match method {
            "POST" => client.post(&self.endpoint),
            "GET" => client.get(&self.endpoint),
            _ => {
                return Err(UmicpClientError::HttpError(format!(
                    "Unsupported HTTP method: {}",
                    method
                )));
            }
        };

        // Add headers
        request_builder = request_builder.headers(headers);

        // Add payload for POST requests
        if method == "POST" {
            request_builder = request_builder.json(&payload);
        }

        // Send the request
        let response = request_builder.send().await?;

        // Check if the request was successful
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(self.handle_error_response(status, error_text));
        }

        // Parse the response as JSON
        let response_json: serde_json::Value = response.json().await?;

        Ok(response_json)
    }

    /// Get the endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Check if the client is configured for a specific protocol
    pub fn is_protocol(&self, protocol: &str) -> bool {
        self.endpoint.starts_with(&format!("{}://", protocol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_umicp_client_creation() {
        let client = UmicpClient::new("http://localhost:8080");
        assert_eq!(client.endpoint(), "http://localhost:8080");
    }

    #[test]
    fn test_umicp_client_with_string() {
        let endpoint = "https://api.example.com".to_string();
        let client = UmicpClient::new(endpoint);
        assert_eq!(client.endpoint(), "https://api.example.com");
    }

    #[test]
    fn test_umicp_client_with_owned_string() {
        let endpoint = "wss://stream.example.com".to_string();
        let client = UmicpClient::new(endpoint.clone());
        assert_eq!(client.endpoint(), "wss://stream.example.com");
    }

    #[test]
    fn test_is_protocol_http() {
        let client = UmicpClient::new("http://localhost:8080");
        assert!(client.is_protocol("http"));
        assert!(!client.is_protocol("https"));
        assert!(!client.is_protocol("ws"));
    }

    #[test]
    fn test_is_protocol_https() {
        let client = UmicpClient::new("https://api.example.com");
        assert!(client.is_protocol("https"));
        assert!(!client.is_protocol("http"));
        assert!(!client.is_protocol("ws"));
    }

    #[test]
    fn test_is_protocol_websocket() {
        let client = UmicpClient::new("ws://localhost:8080");
        assert!(client.is_protocol("ws"));
        assert!(!client.is_protocol("http"));
        assert!(!client.is_protocol("https"));
    }

    #[test]
    fn test_is_protocol_secure_websocket() {
        let client = UmicpClient::new("wss://secure.example.com");
        assert!(client.is_protocol("wss"));
        assert!(!client.is_protocol("ws"));
        assert!(!client.is_protocol("http"));
    }

    #[tokio::test]
    async fn test_umicp_client_request_implementation() {
        let client = UmicpClient::new("http://httpbin.org/post");
        let payload = serde_json::json!({
            "method": "POST",
            "data": "test_value"
        });

        // This should now work with the implementation
        let result = client.request(payload).await;

        // The request might fail due to network, but it shouldn't panic
        // We just verify it doesn't panic with todo! macro
        match result {
            Ok(_) => println!("Request succeeded"),
            Err(e) => println!("Request failed as expected: {}", e),
        }
    }

    #[test]
    fn test_umicp_client_with_api_key() {
        let client = UmicpClient::with_api_key("http://localhost:8080", "nx_test123456789");
        assert_eq!(client.endpoint(), "http://localhost:8080");
    }

    #[tokio::test]
    async fn test_umicp_client_key_rotation() {
        let mut client = UmicpClient::with_api_key("http://localhost:8080", "nx_old_key");
        client.rotate_key("nx_new_key").await;
        // Key rotation should succeed without panicking
    }
}
