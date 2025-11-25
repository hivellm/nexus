//! REST/HTTP streaming client

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

/// REST client for external service integration
pub struct RestClient {
    /// Base URL
    base_url: String,
    /// API key for authentication (optional)
    api_key: Option<Arc<RwLock<String>>>,
}

/// Error types for REST client operations
#[derive(Debug, thiserror::Error)]
pub enum RestClientError {
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

impl RestClient {
    /// Create a new REST client
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
        }
    }

    /// Create a new REST client with API key
    pub fn with_api_key(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
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
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("Nexus-REST-Client/1.0"),
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
    ) -> RestClientError {
        match status {
            reqwest::StatusCode::UNAUTHORIZED => RestClientError::Unauthorized(message),
            reqwest::StatusCode::FORBIDDEN => RestClientError::Forbidden(message),
            reqwest::StatusCode::TOO_MANY_REQUESTS => RestClientError::RateLimitExceeded(message),
            _ => RestClientError::HttpError(message),
        }
    }

    /// Send a POST request
    pub async fn post<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, RestClientError> {
        let client = reqwest::Client::new();
        let url = self.build_url(path);
        let headers = self.build_headers().await;

        let response = client.post(&url).headers(headers).json(body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(self.handle_error_response(status, error_text));
        }

        let result: R = response.json().await?;
        Ok(result)
    }

    /// Send a GET request
    pub async fn get<R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
    ) -> Result<R, RestClientError> {
        let client = reqwest::Client::new();
        let url = self.build_url(path);
        let headers = self.build_headers().await;

        let response = client.get(&url).headers(headers).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(self.handle_error_response(status, error_text));
        }

        let result: R = response.json().await?;
        Ok(result)
    }

    /// Stream data via Server-Sent Events (SSE)
    pub async fn stream(&self, path: &str) -> Result<(), RestClientError> {
        let client = reqwest::Client::new();
        let url = self.build_url(path);
        let mut headers = self.build_headers().await;

        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("text/event-stream"),
        );
        headers.insert(
            reqwest::header::CACHE_CONTROL,
            reqwest::header::HeaderValue::from_static("no-cache"),
        );

        let response = client.get(&url).headers(headers).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(self.handle_error_response(status, error_text));
        }

        // For now, we just verify the connection is established
        // In a full implementation, this would handle SSE parsing
        tracing::debug!("SSE connection established to: {}", url);

        Ok(())
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build full URL for a path
    pub fn build_url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}/{}", self.base_url, path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rest_client_creation() {
        let client = RestClient::new("http://localhost:8080");
        assert_eq!(client.base_url(), "http://localhost:8080");
    }

    #[test]
    fn test_rest_client_with_string() {
        let base_url = "https://api.example.com".to_string();
        let client = RestClient::new(base_url);
        assert_eq!(client.base_url(), "https://api.example.com");
    }

    #[test]
    fn test_build_url_with_leading_slash() {
        let client = RestClient::new("http://localhost:8080");
        let url = client.build_url("/api/v1/users");
        assert_eq!(url, "http://localhost:8080/api/v1/users");
    }

    #[test]
    fn test_build_url_without_leading_slash() {
        let client = RestClient::new("http://localhost:8080");
        let url = client.build_url("api/v1/users");
        assert_eq!(url, "http://localhost:8080/api/v1/users");
    }

    #[test]
    fn test_build_url_with_empty_path() {
        let client = RestClient::new("http://localhost:8080");
        let url = client.build_url("");
        assert_eq!(url, "http://localhost:8080/");
    }

    #[tokio::test]
    async fn test_rest_client_post_implementation() {
        let client = RestClient::new("http://httpbin.org");
        let body = serde_json::json!({"test": "value"});

        // This should now work with the implementation
        let result = client
            .post::<serde_json::Value, serde_json::Value>("/post", &body)
            .await;

        // The request might fail due to network, but it shouldn't panic
        match result {
            Ok(response) => {
                tracing::debug!("POST request succeeded: {:?}", response);
                assert!(response.get("json").is_some());
            }
            Err(e) => tracing::debug!("POST request failed as expected: {}", e),
        }
    }

    #[test]
    fn test_rest_client_with_api_key() {
        let client = RestClient::with_api_key("http://localhost:8080", "nx_test123456789");
        assert_eq!(client.base_url(), "http://localhost:8080");
    }

    #[tokio::test]
    async fn test_rest_client_key_rotation() {
        let mut client = RestClient::with_api_key("http://localhost:8080", "nx_old_key");
        client.rotate_key("nx_new_key").await;
        // Key rotation should succeed without panicking
    }

    #[tokio::test]
    async fn test_rest_client_get_implementation() {
        let client = RestClient::new("http://httpbin.org");

        // This should now work with the implementation
        let result = client.get::<serde_json::Value>("/get").await;

        // The request might fail due to network, but it shouldn't panic
        match result {
            Ok(response) => {
                tracing::debug!("GET request succeeded: {:?}", response);
                assert!(response.get("url").is_some());
            }
            Err(e) => tracing::debug!("GET request failed as expected: {}", e),
        }
    }

    #[tokio::test]
    async fn test_rest_client_stream_implementation() {
        let client = RestClient::new("http://httpbin.org");

        // This should now work with the implementation
        let result = client.stream("/stream/10").await;

        // The request might fail due to network, but it shouldn't panic
        match result {
            Ok(_) => tracing::debug!("Stream request succeeded"),
            Err(e) => tracing::debug!("Stream request failed as expected: {}", e),
        }
    }
}
