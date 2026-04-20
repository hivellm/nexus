//! HTTP bench client. Targets Nexus's `/cypher` REST endpoint.
//!
//! Does not instantiate a Nexus engine. It speaks HTTP/JSON to a
//! server the operator has already started. Every request is bounded
//! by `tokio::time::timeout` — the client cannot hang the runtime.
//! A startup health probe verifies the server is reachable before
//! the harness runs any measured iterations.

use std::time::Duration;

use serde::Deserialize;
use tokio::runtime::Handle;

use super::{BenchClient, ClientError, ExecOutcome, Row};

/// HTTP bench client that targets Nexus's `/cypher` endpoint.
pub struct HttpClient {
    base_url: String,
    engine_label: String,
    http: reqwest::Client,
    runtime: Handle,
}

impl HttpClient {
    /// Connect to a running server. Performs a `GET /health` probe
    /// before returning; if the server doesn't respond within 2
    /// seconds the client fails fast instead of silently proceeding
    /// to a benchmark that never terminates.
    ///
    /// `engine_label` is the column header in the report — typically
    /// `"nexus"` for the Nexus base URL and `"nexus-compat"` or
    /// similar when pointing at a compat bridge.
    pub async fn connect(
        base_url: impl Into<String>,
        engine_label: impl Into<String>,
        runtime: Handle,
    ) -> Result<Self, ClientError> {
        let base_url = base_url.into();
        let engine_label = engine_label.into();

        let http = reqwest::Client::builder()
            // Short global timeout so a slow / hung server never
            // lets a request linger.
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ClientError::Transport(e.to_string()))?;

        let health_url = format!("{}/health", base_url.trim_end_matches('/'));
        let probe = tokio::time::timeout(Duration::from_secs(2), http.get(&health_url).send())
            .await
            .map_err(|_| ClientError::HealthProbe("timed out after 2 s".into()))?
            .map_err(|e| ClientError::HealthProbe(e.to_string()))?;
        if !probe.status().is_success() {
            return Err(ClientError::HealthProbe(format!("HTTP {}", probe.status())));
        }

        Ok(Self {
            base_url,
            engine_label,
            http,
            runtime,
        })
    }

    /// Runtime handle used to bridge the sync [`BenchClient`]
    /// contract into async reqwest calls.
    pub fn runtime(&self) -> &Handle {
        &self.runtime
    }

    /// Shared execute implementation — called from the sync
    /// [`BenchClient`] impl.
    async fn execute_async(
        &self,
        cypher: &str,
        timeout: Duration,
    ) -> Result<ExecOutcome, ClientError> {
        #[derive(serde::Serialize)]
        struct Req<'a> {
            query: &'a str,
        }
        #[derive(Deserialize)]
        struct Resp {
            rows: Vec<Row>,
        }
        let url = format!("{}/cypher", self.base_url.trim_end_matches('/'));
        let send_fut = self.http.post(&url).json(&Req { query: cypher }).send();
        let resp = tokio::time::timeout(timeout, send_fut)
            .await
            .map_err(|_| ClientError::Timeout(timeout))?
            .map_err(|e| ClientError::Transport(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".into());
            return Err(ClientError::Http {
                status: status.as_u16(),
                body,
            });
        }
        let body: Resp = resp
            .json()
            .await
            .map_err(|e| ClientError::BadResponse(e.to_string()))?;
        Ok(ExecOutcome { rows: body.rows })
    }
}

impl BenchClient for HttpClient {
    fn engine_name(&self) -> &str {
        &self.engine_label
    }

    fn execute(&mut self, cypher: &str, timeout: Duration) -> Result<ExecOutcome, ClientError> {
        let cypher = cypher.to_string();
        tokio::task::block_in_place(|| {
            self.runtime
                .block_on(async { self.execute_async(&cypher, timeout).await })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time smoke that the trait bounds line up — the
    /// `HttpClient` is `Send + Sync` and satisfies [`BenchClient`].
    #[test]
    fn http_client_is_send_sync_benchclient() {
        fn assert_traits<T: BenchClient + Send + Sync + 'static>() {}
        assert_traits::<HttpClient>();
    }
}
