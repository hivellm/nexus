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
}
