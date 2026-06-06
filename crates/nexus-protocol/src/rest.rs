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
    /// Shared HTTP client. Built ONCE and reused across every request so the
    /// connection pool actually pools — constructing a fresh `reqwest::Client`
    /// per request gave each call its own (empty) pool, forcing a new TCP
    /// connection every time. Under sustained writes on Windows those
    /// connections piled up in TIME_WAIT and drained the ephemeral port range
    /// (socket exhaustion). `reqwest::Client` is cheap to clone (Arc inside).
    client: reqwest::Client,
}

/// Build the shared HTTP client with keep-alive and a bounded idle pool so
/// repeated requests reuse connections instead of opening a new socket each
/// time. Falls back to the default client if the builder fails (never panics).
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(32)
        .pool_idle_timeout(Some(std::time::Duration::from_secs(90)))
        .tcp_keepalive(Some(std::time::Duration::from_secs(60)))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
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
            client: build_http_client(),
        }
    }

    /// Create a new REST client with API key
    pub fn with_api_key(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: Some(Arc::new(RwLock::new(api_key.into()))),
            client: build_http_client(),
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
        let url = self.build_url(path);
        let headers = self.build_headers().await;

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(body)
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

        let result: R = response.json().await?;
        Ok(result)
    }

    /// Send a GET request
    pub async fn get<R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
    ) -> Result<R, RestClientError> {
        let url = self.build_url(path);
        let headers = self.build_headers().await;

        let response = self.client.get(&url).headers(headers).send().await?;

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

        let response = self.client.get(&url).headers(headers).send().await?;

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

    /// GH #6 transport fix — the RestClient must reuse one pooled keep-alive
    /// connection across requests. The previous code built a fresh
    /// `reqwest::Client` per request, so each call opened a new TCP connection
    /// (socket exhaustion on Windows under sustained writes). This drives a
    /// minimal keep-alive server that counts accepted connections: five
    /// sequential POSTs must share (essentially) one connection.
    #[tokio::test]
    async fn reuses_connection_across_sequential_requests() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let conns = Arc::new(AtomicUsize::new(0));
        let conns_srv = conns.clone();

        // Minimal HTTP/1.1 keep-alive responder: count each accepted
        // connection, then loop serving a fixed JSON body on the same socket.
        let server = tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => break,
                };
                conns_srv.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = [0u8; 8192];
                    loop {
                        match sock.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(_) => {}
                        }
                        let body = b"{}";
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                             Content-Length: {}\r\nConnection: keep-alive\r\n\r\n",
                            body.len()
                        );
                        if sock.write_all(head.as_bytes()).await.is_err()
                            || sock.write_all(body).await.is_err()
                        {
                            break;
                        }
                        let _ = sock.flush().await;
                    }
                });
            }
        });

        let client = RestClient::new(format!("http://{addr}"));
        for _ in 0..5 {
            let res = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                client.post::<_, serde_json::Value>("/x", &serde_json::json!({"k": 1})),
            )
            .await
            .expect("request must not hang");
            res.expect("POST must succeed against the keep-alive server");
        }

        server.abort();

        let observed = conns.load(Ordering::SeqCst);
        assert!(
            observed <= 2,
            "expected connection reuse (<=2 connections for 5 requests), got {observed}"
        );
    }
}
