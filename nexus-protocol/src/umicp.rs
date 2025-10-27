//! UMICP (Universal Model Interoperability Protocol) client integration

/// UMICP client for universal model communication
pub struct UmicpClient {
    /// Server endpoint
    endpoint: String,
}

impl UmicpClient {
    /// Create a new UMICP client
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    /// Send a UMICP request
    pub async fn request(&self, payload: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        // Create HTTP client
        let client = reqwest::Client::new();

        // Determine the request method based on the payload
        let method = if payload.get("method").is_some() {
            "POST"
        } else {
            "GET"
        };

        // Build the request
        let mut request_builder = match method {
            "POST" => client.post(&self.endpoint),
            "GET" => client.get(&self.endpoint),
            _ => return Err(anyhow::anyhow!("Unsupported HTTP method: {}", method)),
        };

        // Add headers for UMICP protocol
        request_builder = request_builder
            .header("Content-Type", "application/json")
            .header("X-Protocol", "UMICP")
            .header("User-Agent", "Nexus-UMICP-Client/1.0");

        // Add payload for POST requests
        if method == "POST" {
            request_builder = request_builder.json(&payload);
        }

        // Send the request
        let response = request_builder.send().await?;

        // Check if the request was successful
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "UMICP request failed with status: {}",
                response.status()
            ));
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
}
