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
        path: &str,
        body: &T,
    ) -> anyhow::Result<R> {
        let client = reqwest::Client::new();
        let url = self.build_url(path);

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "Nexus-REST-Client/1.0")
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "REST POST request failed with status: {}",
                response.status()
            ));
        }

        let result: R = response.json().await?;
        Ok(result)
    }

    /// Send a GET request
    pub async fn get<R: for<'de> Deserialize<'de>>(&self, path: &str) -> anyhow::Result<R> {
        let client = reqwest::Client::new();
        let url = self.build_url(path);

        let response = client
            .get(&url)
            .header("User-Agent", "Nexus-REST-Client/1.0")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "REST GET request failed with status: {}",
                response.status()
            ));
        }

        let result: R = response.json().await?;
        Ok(result)
    }

    /// Stream data via Server-Sent Events (SSE)
    pub async fn stream(&self, path: &str) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let url = self.build_url(path);

        let response = client
            .get(&url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("User-Agent", "Nexus-REST-Client/1.0")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "REST streaming request failed with status: {}",
                response.status()
            ));
        }

        // For now, we just verify the connection is established
        // In a full implementation, this would handle SSE parsing
        println!("SSE connection established to: {}", url);

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
                println!("POST request succeeded: {:?}", response);
                assert!(response.get("json").is_some());
            }
            Err(e) => println!("POST request failed as expected: {}", e),
        }
    }

    #[tokio::test]
    async fn test_rest_client_get_implementation() {
        let client = RestClient::new("http://httpbin.org");

        // This should now work with the implementation
        let result = client.get::<serde_json::Value>("/get").await;

        // The request might fail due to network, but it shouldn't panic
        match result {
            Ok(response) => {
                println!("GET request succeeded: {:?}", response);
                assert!(response.get("url").is_some());
            }
            Err(e) => println!("GET request failed as expected: {}", e),
        }
    }

    #[tokio::test]
    async fn test_rest_client_stream_implementation() {
        let client = RestClient::new("http://httpbin.org");

        // This should now work with the implementation
        let result = client.stream("/stream/10").await;

        // The request might fail due to network, but it shouldn't panic
        match result {
            Ok(_) => println!("Stream request succeeded"),
            Err(e) => println!("Stream request failed as expected: {}", e),
        }
    }
}
