//! HTTP benchmark client. Feature-gated on `live-bench`.
//!
//! **Does NOT** instantiate a Nexus engine. It speaks HTTP/JSON to a
//! server the operator has already started. Every request is bounded
//! by `tokio::time::timeout` — the client cannot hang the runtime.
//! A startup `health_check` verifies the server is reachable before
//! the harness runs any measured iterations.

use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;
use tokio::runtime::Handle;

use crate::harness::{BenchExecute, ExecResult};

/// Row shape matching Nexus's `/cypher` REST response. Kept local so
/// the no-live-bench build doesn't pull in anything extra.
pub type Row = Vec<serde_json::Value>;

/// Minimal outcome the client publishes; the harness converts to
/// `ExecResult` internally.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecOutcome {
    /// Rows returned by the engine, in order.
    pub rows: Vec<Row>,
}

/// Errors the HTTP client can surface.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Transport-level failure: connect refused, DNS, TLS.
    #[error("transport error: {0}")]
    Transport(String),
    /// Server answered but with a non-200 status.
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    /// The server returned JSON that didn't match the expected
    /// shape.
    #[error("malformed response: {0}")]
    BadResponse(String),
    /// Soft per-call timeout elapsed. The harness maps this to a
    /// scenario failure rather than silently recording a huge
    /// latency.
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    /// `/health` probe failed at startup.
    #[error("server /health probe failed: {0}")]
    HealthProbe(String),
}

/// Narrow trait every bench client must satisfy. The harness is
/// generic over this, so the HTTP client + any future non-HTTP
/// client (Bolt, …) plug in without touching the runner.
pub trait BenchClient: Send + Sync {
    /// Label reported in the engine column of the report.
    fn engine_name(&self) -> &str;

    /// Issue a single Cypher request. Must return within `timeout`
    /// or surface [`ClientError::Timeout`].
    fn execute(&mut self, cypher: &str, timeout: Duration) -> Result<ExecOutcome, ClientError>;
}

/// HTTP benchmark client that targets Nexus's `/cypher` endpoint.
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

    /// Runtime handle used to bridge the sync `BenchExecute` contract
    /// into async reqwest calls.
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

/// Bridge from the rich [`BenchClient`] trait to the harness's
/// narrower [`BenchExecute`] contract.
impl<T: BenchClient + ?Sized> BenchExecute for &mut T {
    fn execute(
        &mut self,
        cypher: &str,
        timeout: Duration,
    ) -> Result<ExecResult, Box<dyn std::error::Error + Send + Sync>> {
        let out = BenchClient::execute(*self, cypher, timeout)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        Ok(ExecResult {
            row_count: out.rows.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time smoke that the trait bounds line up — the
    /// `HttpClient` is `Send + Sync` and satisfies both traits.
    #[test]
    fn http_client_is_send_sync_benchclient() {
        fn assert_traits<T: BenchClient + Send + Sync + 'static>() {}
        assert_traits::<HttpClient>();
    }

    #[test]
    fn exec_outcome_row_count_matches_vec_len() {
        let out = ExecOutcome {
            rows: vec![vec![serde_json::Value::from(1)]; 4],
        };
        assert_eq!(out.rows.len(), 4);
    }
}
