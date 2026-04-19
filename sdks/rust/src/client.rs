//! Nexus client implementation.
//!
//! Every public method on `NexusClient` routes through a
//! [`crate::transport::Transport`] picked at construction time
//! (RPC for `nexus://` URLs, HTTP for `http://` / `https://`,
//! overridable via `ClientConfig.transport` or the
//! `NEXUS_SDK_TRANSPORT` env var). The method signatures here match
//! what the SDK shipped before `phase2_sdk-rpc-transport-default` so
//! user code compiles unchanged.

use crate::error::{NexusError, Result};
use crate::models::*;
use crate::transport::endpoint::Endpoint;
use crate::transport::http::{HttpCredentials, HttpTransport, nexus_to_json};
use crate::transport::rpc::{RpcCredentials, RpcTransport};
use crate::transport::{Transport, TransportMode, TransportRequest};
use base64::Engine;
use nexus_protocol::rpc::types::NexusValue;
use reqwest::{Client, ClientBuilder, Response};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

/// Nexus client — transport-agnostic handle to a running server.
#[derive(Clone)]
pub struct NexusClient {
    /// Active transport. `Arc` so `Clone` is cheap and independent
    /// of whether the underlying connection is shared state.
    transport: Arc<dyn Transport>,
    /// Raw HTTP client — retained for a handful of legacy manager
    /// methods (multi-database HTTP helpers) that have not yet been
    /// ported to the Transport trait. New code SHOULD use
    /// `self.transport.execute(...)` instead.
    client: Client,
    /// HTTP base URL derived from the endpoint. Used only by the
    /// legacy paths above.
    base_url: Url,
    api_key: Option<String>,
    username: Option<String>,
    password: Option<String>,
    #[allow(dead_code)]
    max_retries: u32,
}

impl std::fmt::Debug for NexusClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NexusClient")
            .field("transport", &self.transport.describe())
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl NexusClient {
    pub(crate) fn get_client(&self) -> &Client {
        &self.client
    }

    pub(crate) fn get_base_url(&self) -> &Url {
        &self.base_url
    }

    /// Describe the active transport (`"nexus://host:15475 (RPC)"`).
    /// Useful for tracing and `--verbose`-style diagnostics in
    /// applications wrapping the SDK.
    pub fn endpoint_description(&self) -> String {
        self.transport.describe()
    }

    /// True when the active transport uses the native binary RPC
    /// wire format.
    pub fn is_rpc(&self) -> bool {
        self.transport.is_rpc()
    }

    /// Create a client with default configuration against `base_url`.
    ///
    /// `base_url` may use `nexus://`, `http://`, `https://`, or a bare
    /// `host:port` (defaults to RPC). Loading a `nexus://` URL picks
    /// the native binary RPC transport; any other scheme uses HTTP.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::NexusClient;
    ///
    /// // Binary RPC (fastest path)
    /// let rpc = NexusClient::new("nexus://localhost:15475")?;
    ///
    /// // Legacy HTTP
    /// let http = NexusClient::new("http://localhost:15474")?;
    /// # Ok::<(), nexus_sdk::NexusError>(())
    /// ```
    pub fn new(base_url: &str) -> Result<Self> {
        Self::with_config(ClientConfig {
            base_url: base_url.to_string(),
            ..Default::default()
        })
    }

    /// Create a client with API key authentication.
    pub fn with_api_key(base_url: &str, api_key: &str) -> Result<Self> {
        Self::with_config(ClientConfig {
            base_url: base_url.to_string(),
            api_key: Some(api_key.to_string()),
            ..Default::default()
        })
    }

    /// Create a client with username/password authentication.
    pub fn with_credentials(base_url: &str, username: &str, password: &str) -> Result<Self> {
        Self::with_config(ClientConfig {
            base_url: base_url.to_string(),
            username: Some(username.to_string()),
            password: Some(password.to_string()),
            ..Default::default()
        })
    }

    /// Create a client from a custom [`ClientConfig`].
    ///
    /// Transport precedence (strongest → weakest):
    ///
    /// 1. URL scheme in `config.base_url` (`nexus://` forces RPC,
    ///    `http[s]://` forces HTTP).
    /// 2. `NEXUS_SDK_TRANSPORT` env var.
    /// 3. `config.transport` field.
    /// 4. Default: `TransportMode::NexusRpc`.
    pub fn with_config(config: ClientConfig) -> Result<Self> {
        let endpoint = Endpoint::parse(&config.base_url)?;
        let url_force = endpoint.scheme;

        // Env var overrides the config field but not the URL scheme.
        let env_mode = std::env::var("NEXUS_SDK_TRANSPORT")
            .ok()
            .and_then(|s| TransportMode::parse(&s));
        let effective_mode = match url_force {
            crate::transport::endpoint::Scheme::Rpc => Some(TransportMode::NexusRpc),
            crate::transport::endpoint::Scheme::Http => Some(TransportMode::Http),
            crate::transport::endpoint::Scheme::Https => Some(TransportMode::Https),
            crate::transport::endpoint::Scheme::Resp3 => Some(TransportMode::Resp3),
        };
        let mode = effective_mode
            .or(env_mode)
            .or(config.transport)
            .unwrap_or(TransportMode::NexusRpc);

        // Build the legacy HTTP client regardless of active mode —
        // a handful of manager methods still hit REST directly
        // until their RPC verbs land.
        let timeout = Duration::from_secs(config.timeout_secs);
        let http_client_builder = ClientBuilder::new()
            .timeout(timeout)
            .user_agent(format!("nexus-sdk/{}", env!("CARGO_PKG_VERSION")));
        let http_client = http_client_builder.build()?;

        // Parse the base_url into a `url::Url` for the legacy path.
        // `nexus://` isn't a scheme `url::Url` knows as HTTP-like, so
        // synthesise an HTTP-equivalent URL for that storage.
        let url_for_legacy = match mode {
            TransportMode::NexusRpc | TransportMode::Resp3 => endpoint.as_http_url(),
            TransportMode::Http | TransportMode::Https => config.base_url.clone(),
        };
        let base_url = Url::parse(&url_for_legacy)
            .map_err(|e| NexusError::Configuration(format!("Invalid base URL: {}", e)))?;

        // Build the transport per the resolved mode.
        let transport: Arc<dyn Transport> = match mode {
            TransportMode::NexusRpc => Arc::new(RpcTransport::new(
                endpoint.clone(),
                RpcCredentials {
                    api_key: config.api_key.clone(),
                    username: config.username.clone(),
                    password: config.password.clone(),
                },
            )),
            TransportMode::Http | TransportMode::Https => Arc::new(HttpTransport::new(
                endpoint.clone(),
                HttpCredentials {
                    api_key: config.api_key.clone(),
                    username: config.username.clone(),
                    password: config.password.clone(),
                },
                config.timeout_secs,
            )?),
            TransportMode::Resp3 => {
                return Err(NexusError::Configuration(
                    "RESP3 transport is not yet implemented in the Rust SDK \
                     (see phase2_sdk-rpc-transport-default §2.3). \
                     Use 'nexus://' for binary RPC or 'http://' for HTTP/JSON."
                        .to_string(),
                ));
            }
        };

        Ok(Self {
            transport,
            client: http_client,
            base_url,
            api_key: config.api_key,
            username: config.username,
            password: config.password,
            max_retries: config.max_retries,
        })
    }

    // ══ Transport-routed methods ═════════════════════════════════════════════
    // Every method below goes through `self.transport.execute(...)`. The RPC
    // path hits `nexus-server/src/protocol/rpc/dispatch/*`; the HTTP path hits
    // the sibling REST handler. See `docs/specs/sdk-transport.md`.

    /// Execute a Cypher query.
    pub async fn execute_cypher(
        &self,
        query: &str,
        parameters: Option<HashMap<String, Value>>,
    ) -> Result<QueryResult> {
        let mut args = vec![NexusValue::Str(query.to_string())];
        if let Some(params) = parameters {
            let mut pairs = Vec::with_capacity(params.len());
            for (k, v) in params {
                pairs.push((NexusValue::Str(k), value_to_nexus(v)));
            }
            args.push(NexusValue::Map(pairs));
        }
        let resp = self
            .transport
            .execute(TransportRequest {
                command: "CYPHER".to_string(),
                args,
            })
            .await?;
        cypher_envelope_to_query_result(resp.value)
    }

    /// Get database statistics.
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        let resp = self
            .transport
            .execute(TransportRequest {
                command: "STATS".to_string(),
                args: vec![],
            })
            .await?;
        // The RPC STATS envelope is flat (`{nodes, relationships,
        // labels, rel_types, page_cache_hits/misses, wal_entries,
        // active_transactions}`); the REST shape is nested
        // (`{catalog: {node_count, ...}, label_index: {...}}`). The
        // SDK's public `DatabaseStats` type is the nested shape —
        // synthesise it from whichever form arrived so callers see a
        // consistent struct regardless of transport.
        let json = nexus_to_json(&resp.value);
        let obj = json.as_object().ok_or_else(|| {
            NexusError::Network(format!("STATS reply must be a map, got {:?}", resp.value))
        })?;

        // REST path: serde straight through.
        if obj.contains_key("catalog") {
            return Ok(serde_json::from_value(json)?);
        }

        // RPC flat path: hand-populate `CatalogStats` from the RPC
        // field names. Legacy fields we no longer expose via RPC
        // (`label_index`, `knn_index`) fall back to empty defaults.
        let u = |k: &str| obj.get(k).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let catalog = CatalogStats {
            node_count: u("nodes"),
            rel_count: u("relationships"),
            label_count: u("labels"),
            rel_type_count: u("rel_types"),
        };
        Ok(DatabaseStats {
            catalog,
            label_index: LabelIndexStats::default(),
            knn_index: KnnIndexStats::default(),
        })
    }

    /// Health check — returns `true` if the server responds
    /// successfully on the active transport.
    pub async fn health_check(&self) -> Result<bool> {
        match self
            .transport
            .execute(TransportRequest {
                command: if self.is_rpc() { "PING" } else { "HEALTH" }.to_string(),
                args: vec![],
            })
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Add authentication headers to an HTTP-fallback request.
    /// Retained for compatibility with the multi-database manager
    /// methods below; new code does NOT need this.
    pub(crate) fn add_auth_headers(
        &self,
        mut builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder> {
        if let Some(api_key) = &self.api_key {
            builder = builder.header("X-API-Key", api_key);
        } else if let (Some(username), Some(password)) = (&self.username, &self.password) {
            let auth = base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", username, password));
            builder = builder.header("Authorization", format!("Basic {}", auth));
        }
        Ok(builder)
    }

    /// Execute an HTTP-fallback request with retries. Retained for
    /// the manager methods that still use REST directly.
    pub(crate) async fn execute_with_retry(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<Response> {
        let max_retries = self.max_retries;
        let mut last_error = None;

        for attempt in 0..=max_retries {
            match builder.try_clone() {
                Some(cloned_builder) => match cloned_builder.send().await {
                    Ok(response) => {
                        let status = response.status();
                        if status.is_server_error() && attempt < max_retries {
                            let delay_ms = 100u64 * (1u64 << attempt.min(5));
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                            continue;
                        }
                        return Ok(response);
                    }
                    Err(e) => {
                        let is_retryable = e.is_timeout() || e.is_connect() || e.is_request();
                        last_error = Some(e);
                        if is_retryable && attempt < max_retries {
                            let delay_ms = 100u64 * (1u64 << attempt.min(5));
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                            continue;
                        }
                        break;
                    }
                },
                None => {
                    return builder.send().await.map_err(NexusError::Http);
                }
            }
        }

        match last_error {
            Some(e) => Err(NexusError::Http(e)),
            None => Err(NexusError::Network(
                "Request failed after retries".to_string(),
            )),
        }
    }

    // =========================================================================
    // Legacy database-management methods — still REST-only.
    // These are kept as-is because the server-side /databases/* routes
    // were not part of phase 2's RPC surface; adding them is tracked in
    // a follow-up.
    // =========================================================================

    /// List all databases (REST).
    pub async fn list_databases(&self) -> Result<ListDatabasesResponse> {
        let url = self.get_base_url().join("/databases")?;
        let mut request_builder = self.get_client().get(url);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: ListDatabasesResponse = response.json().await?;
        Ok(result)
    }

    /// Create a new database (REST).
    pub async fn create_database(&self, name: &str) -> Result<CreateDatabaseResponse> {
        let url = self.get_base_url().join("/databases")?;
        let request = CreateDatabaseRequest {
            name: name.to_string(),
        };
        let mut request_builder = self.get_client().post(url).json(&request);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: CreateDatabaseResponse = response.json().await?;
        Ok(result)
    }

    /// Get database information (REST).
    pub async fn get_database(&self, name: &str) -> Result<DatabaseInfo> {
        let url = self.get_base_url().join(&format!("/databases/{}", name))?;
        let mut request_builder = self.get_client().get(url);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: DatabaseInfo = response.json().await?;
        Ok(result)
    }

    /// Drop a database (REST).
    pub async fn drop_database(&self, name: &str) -> Result<DropDatabaseResponse> {
        let url = self.get_base_url().join(&format!("/databases/{}", name))?;
        let mut request_builder = self.get_client().delete(url);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: DropDatabaseResponse = response.json().await?;
        Ok(result)
    }

    /// Get the current session database (REST).
    pub async fn get_current_database(&self) -> Result<String> {
        let url = self.get_base_url().join("/session/database")?;
        let mut request_builder = self.get_client().get(url);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: SessionDatabaseResponse = response.json().await?;
        Ok(result.database)
    }

    /// Switch to a different database (REST).
    pub async fn switch_database(&self, name: &str) -> Result<SwitchDatabaseResponse> {
        let url = self.get_base_url().join("/session/database")?;
        let request = SwitchDatabaseRequest {
            name: name.to_string(),
        };
        let mut request_builder = self.get_client().put(url).json(&request);
        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let result: SwitchDatabaseResponse = response.json().await?;
        Ok(result)
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Map the SDK's `Value` type onto `NexusValue`. `Value` already
/// carries the same shape as `serde_json::Value`, so we serialise
/// through that and reuse the HTTP transport's JSON→Nexus helper.
fn value_to_nexus(v: Value) -> NexusValue {
    let json = serde_json::to_value(&v).unwrap_or(serde_json::Value::Null);
    crate::transport::http::json_to_nexus(json)
}

/// Decode the CYPHER reply envelope (`{columns, rows, execution_time_ms, error}`)
/// into `QueryResult`.
fn cypher_envelope_to_query_result(value: NexusValue) -> Result<QueryResult> {
    let json = nexus_to_json(&value);
    let obj = json.as_object().ok_or_else(|| {
        NexusError::Network(format!("CYPHER reply must be a map, got {:?}", value))
    })?;

    let columns = obj
        .get("columns")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let rows = obj
        .get("rows")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let execution_time_ms = obj.get("execution_time_ms").and_then(|v| v.as_u64());
    let error = obj.get("error").and_then(|v| v.as_str()).map(String::from);

    if let Some(msg) = error.clone() {
        return Err(NexusError::Api {
            message: msg,
            status: 0,
        });
    }

    Ok(QueryResult {
        columns,
        rows,
        execution_time_ms,
        error,
    })
}
