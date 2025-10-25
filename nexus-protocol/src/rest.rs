//! REST/HTTP streaming client

use serde::{Deserialize, Serialize};

/// REST client for external service integration
pub struct RestClient {
    /// Base URL
    base_url: String,
}

impl RestClient {
    /// Create a new REST client
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    /// Send a POST request
    pub async fn post<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        _path: &str,
        _body: &T,
    ) -> anyhow::Result<R> {
        todo!("REST POST - to be implemented")
    }

    /// Send a GET request
    pub async fn get<R: for<'de> Deserialize<'de>>(&self, _path: &str) -> anyhow::Result<R> {
        todo!("REST GET - to be implemented")
    }

    /// Stream data via Server-Sent Events (SSE)
    pub async fn stream(&self, _path: &str) -> anyhow::Result<()> {
        todo!("REST streaming - to be implemented")
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
    async fn test_rest_client_post_todo() {
        let client = RestClient::new("http://localhost:8080");
        let body = serde_json::json!({"test": "value"});
        
        // This should panic with todo! macro
        let result = std::panic::catch_unwind(|| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(
                    client.post::<serde_json::Value, serde_json::Value>("/test", &body)
                )
            })
        });
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rest_client_get_todo() {
        let client = RestClient::new("http://localhost:8080");
        
        // This should panic with todo! macro
        let result = std::panic::catch_unwind(|| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(
                    client.get::<serde_json::Value>("/test")
                )
            })
        });
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rest_client_stream_todo() {
        let client = RestClient::new("http://localhost:8080");
        
        // This should panic with todo! macro
        let result = std::panic::catch_unwind(|| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(
                    client.stream("/stream")
                )
            })
        });
        
        assert!(result.is_err());
    }
}
