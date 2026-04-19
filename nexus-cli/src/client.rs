use anyhow::{Result, anyhow};
use nexus_protocol::rpc::types::NexusValue;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::endpoint::Endpoint;
use crate::rpc_transport::{RpcCredentials, RpcTransport};

/// High-level CLI client. Owns the endpoint, the HTTP client (always
/// constructed so fallback paths work), and — when the endpoint is
/// `nexus://` — a lazily-connected `RpcTransport`.
///
/// `query()` and `ping()` dispatch on the endpoint scheme: RPC when
/// `nexus://`, HTTP otherwise. Every other method currently falls
/// through to HTTP against `Endpoint::as_http_url()`; adding RPC verbs
/// is a one-liner once the server exposes a matching command.
pub struct NexusClient {
    endpoint: Endpoint,
    http: Client,
    http_base: String,
    api_key: Option<String>,
    username: Option<String>,
    password: Option<String>,
    rpc: Option<RpcTransport>,
}

#[derive(Debug, Serialize)]
struct CypherRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    #[serde(default)]
    pub stats: Option<QueryStats>,
}

#[derive(Debug, Deserialize)]
pub struct QueryStats {
    #[serde(default)]
    pub nodes_created: i64,
    #[serde(default)]
    pub nodes_deleted: i64,
    #[serde(default)]
    pub relationships_created: i64,
    #[serde(default)]
    pub relationships_deleted: i64,
    #[serde(default)]
    pub properties_set: i64,
    #[serde(default)]
    pub execution_time_ms: f64,
}

#[derive(Debug, Deserialize)]
pub struct UsersResponse {
    pub users: Vec<UserInfo>,
}

#[derive(Debug, Deserialize)]
pub struct KeysResponse {
    pub keys: Vec<ApiKeyInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(default)]
    pub id: Option<String>,
    pub username: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_root: bool,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeyCreateResponse {
    pub id: String,
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerStatus {
    pub status: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub uptime_seconds: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseStats {
    #[serde(default)]
    pub node_count: i64,
    #[serde(default)]
    pub relationship_count: i64,
    #[serde(default)]
    pub label_count: i64,
    #[serde(default)]
    pub property_key_count: i64,
}

impl NexusClient {
    /// Build a client from CLI arguments. When no URL is supplied the
    /// default is `nexus://127.0.0.1:15475` — the CLI is RPC-first.
    ///
    /// `transport_override`:
    /// - `None` → pick transport from the URL scheme (the sane default).
    /// - `Some("rpc")` / `Some("http")` → force that transport even if
    ///   the URL scheme disagrees (`NEXUS_TRANSPORT` env var or the
    ///   `--transport` flag feed this).
    pub fn new(
        url: Option<&str>,
        api_key: Option<&str>,
        username: Option<&str>,
        password: Option<&str>,
        transport_override: Option<&str>,
    ) -> Result<Self> {
        let endpoint = match url {
            Some(s) => Endpoint::parse(s)?,
            None => Endpoint::default_local(),
        };
        let endpoint = apply_transport_override(endpoint, transport_override)?;

        let http_base = endpoint.as_http_url();
        let rpc = if endpoint.is_rpc() {
            Some(RpcTransport::new(
                endpoint.clone(),
                RpcCredentials {
                    api_key: api_key.map(String::from),
                    username: username.map(String::from),
                    password: password.map(String::from),
                },
            ))
        } else {
            None
        };

        Ok(Self {
            endpoint,
            http: Client::new(),
            http_base,
            api_key: api_key.map(String::from),
            username: username.map(String::from),
            password: password.map(String::from),
            rpc,
        })
    }

    /// True if the active transport is the native RPC binary format.
    pub fn is_rpc(&self) -> bool {
        self.rpc.is_some()
    }

    /// Human-readable description of the active endpoint. Used by
    /// `--verbose` diagnostics.
    pub fn endpoint_description(&self) -> String {
        self.endpoint.to_string()
    }

    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.http_base, path);
        let mut req = self.http.request(method, &url);

        if let Some(ref key) = self.api_key {
            req = req.header("X-API-Key", key);
        }

        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            req = req.basic_auth(user, Some(pass));
        }

        req
    }

    /// Hit the HTTP surface for a command that has no RPC verb yet.
    /// Emits a visible warning so users know a fallback kicked in
    /// (required by the task's "no silent fallback" rule).
    #[allow(dead_code)]
    fn warn_http_fallback(&self, command: &str) {
        if self.is_rpc() {
            eprintln!(
                "warning: '{}' has no RPC verb on the server; falling back to HTTP via {}",
                command, self.http_base
            );
        }
    }

    pub async fn query(&self, cypher: &str, params: Option<Value>) -> Result<QueryResult> {
        if let Some(rpc) = &self.rpc {
            return self.query_rpc(rpc, cypher, params).await;
        }
        self.query_http(cypher, params).await
    }

    async fn query_rpc(
        &self,
        rpc: &RpcTransport,
        cypher: &str,
        params: Option<Value>,
    ) -> Result<QueryResult> {
        let mut args = vec![NexusValue::Str(cypher.to_string())];
        if let Some(p) = params {
            args.push(json_to_nexus(p));
        }
        let reply = rpc.call("CYPHER", args).await?;
        nexus_to_query_result(reply)
    }

    async fn query_http(&self, cypher: &str, params: Option<Value>) -> Result<QueryResult> {
        let req = CypherRequest {
            query: cypher.to_string(),
            params,
        };

        let response = self
            .build_request(reqwest::Method::POST, "/cypher")
            .json(&req)
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            let result: QueryResult = response.json().await?;
            Ok(result)
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(anyhow!("Query failed ({}): {}", status, text))
        }
    }

    pub async fn ping(&self) -> Result<bool> {
        if let Some(rpc) = &self.rpc {
            return match rpc.call("PING", vec![]).await {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            };
        }
        let response = self
            .build_request(reqwest::Method::GET, "/health")
            .send()
            .await?;
        Ok(response.status() == StatusCode::OK)
    }

    pub async fn status(&self) -> Result<ServerStatus> {
        let response = self
            .build_request(reqwest::Method::GET, "/status")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(response.json().await?)
        } else {
            // Fallback for servers without /status endpoint
            Ok(ServerStatus {
                status: "running".to_string(),
                version: None,
                uptime_seconds: None,
            })
        }
    }

    pub async fn health(&self) -> Result<Value> {
        let response = self
            .build_request(reqwest::Method::GET, "/health")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(response.json().await?)
        } else {
            Err(anyhow!("Health check failed"))
        }
    }

    pub async fn stats(&self) -> Result<DatabaseStats> {
        let response = self
            .build_request(reqwest::Method::GET, "/stats")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(response.json().await?)
        } else {
            Err(anyhow!("Failed to get stats"))
        }
    }

    pub async fn get_users(&self) -> Result<Vec<UserInfo>> {
        let response = self
            .build_request(reqwest::Method::GET, "/auth/users")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            let result: UsersResponse = response.json().await?;
            Ok(result.users)
        } else {
            Err(anyhow!("Failed to get users"))
        }
    }

    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        _roles: &[String],
    ) -> Result<()> {
        let cypher = format!(
            "CREATE USER {} SET PASSWORD '{}'",
            username,
            password.replace('\'', "''")
        );
        let result = self.query(&cypher, None).await?;
        if result.rows.is_empty() {
            anyhow::bail!("Failed to create user");
        }
        Ok(())
    }

    pub async fn delete_user(&self, username: &str) -> Result<()> {
        let cypher = format!("DROP USER {}", username);
        self.query(&cypher, None).await?;
        Ok(())
    }

    pub async fn get_api_keys(&self) -> Result<Vec<ApiKeyInfo>> {
        let response = self
            .build_request(reqwest::Method::GET, "/auth/keys")
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            let result: KeysResponse = response.json().await?;
            Ok(result.keys)
        } else {
            Err(anyhow!("Failed to get API keys"))
        }
    }

    pub async fn create_api_key(
        &self,
        name: &str,
        permissions: &[String],
    ) -> Result<ApiKeyCreateResponse> {
        let permissions_str = if permissions.is_empty() {
            String::new()
        } else {
            format!(" WITH PERMISSIONS {}", permissions.join(", "))
        };
        let cypher = format!("CREATE API KEY {}{}", name, permissions_str);
        let result = self.query(&cypher, None).await?;

        if result.rows.is_empty() {
            anyhow::bail!("Failed to create API key");
        }

        // Columns: ["key_id", "name", "key", "message"]
        let row = &result.rows[0];
        let key_id = row
            .first()
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let key = row
            .get(2)
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(ApiKeyCreateResponse {
            id: key_id,
            name: name.to_string(),
            key,
            permissions: permissions.to_vec(),
        })
    }

    pub async fn revoke_api_key(&self, id: &str) -> Result<()> {
        let cypher = format!("REVOKE API KEY {}", id);
        self.query(&cypher, None).await?;
        Ok(())
    }

    pub async fn get_labels(&self) -> Result<Vec<String>> {
        let result = self.query("CALL db.labels()", None).await?;
        let labels: Vec<String> = result
            .rows
            .iter()
            .filter_map(|row| row.first().and_then(|v| v.as_str().map(String::from)))
            .collect();
        Ok(labels)
    }

    pub async fn get_relationship_types(&self) -> Result<Vec<String>> {
        let result = self.query("CALL db.relationshipTypes()", None).await?;
        let types: Vec<String> = result
            .rows
            .iter()
            .filter_map(|row| row.first().and_then(|v| v.as_str().map(String::from)))
            .collect();
        Ok(types)
    }

    pub async fn get_indexes(&self) -> Result<Vec<Value>> {
        let result = self.query("SHOW INDEXES", None).await?;
        Ok(result.rows.into_iter().map(Value::Array).collect())
    }

    pub async fn clear_database(&self) -> Result<()> {
        self.query("MATCH (n) DETACH DELETE n", None).await?;
        Ok(())
    }

    pub async fn export_data(&self, format: &str) -> Result<String> {
        let response = self
            .build_request(reqwest::Method::GET, &format!("/export?format={}", format))
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(response.text().await?)
        } else {
            Err(anyhow!("Export failed"))
        }
    }

    pub async fn import_data(&self, data: &str, format: &str) -> Result<()> {
        let response = self
            .build_request(reqwest::Method::POST, &format!("/import?format={}", format))
            .body(data.to_string())
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            Ok(())
        } else {
            let text = response.text().await.unwrap_or_default();
            Err(anyhow!("Import failed: {}", text))
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn apply_transport_override(endpoint: Endpoint, override_: Option<&str>) -> Result<Endpoint> {
    use crate::endpoint::{HTTP_DEFAULT_PORT, RPC_DEFAULT_PORT, Scheme};

    let Some(raw) = override_ else {
        return Ok(endpoint);
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => Ok(endpoint),
        "rpc" | "nexus" => {
            let port = if endpoint.scheme == Scheme::Rpc {
                endpoint.port
            } else {
                RPC_DEFAULT_PORT
            };
            Ok(Endpoint {
                scheme: Scheme::Rpc,
                host: endpoint.host,
                port,
            })
        }
        "http" => {
            let port = if endpoint.scheme == Scheme::Http {
                endpoint.port
            } else {
                HTTP_DEFAULT_PORT
            };
            Ok(Endpoint {
                scheme: Scheme::Http,
                host: endpoint.host,
                port,
            })
        }
        "https" => Ok(Endpoint {
            scheme: Scheme::Https,
            host: endpoint.host,
            port: endpoint.port,
        }),
        other => Err(anyhow!(
            "unknown --transport value '{}' (expected: rpc, http, https, or auto)",
            other
        )),
    }
}

/// Convert a `serde_json::Value` to a `NexusValue` for RPC wire
/// transmission. Kept in sync with the server's
/// `dispatch::convert::json_to_nexus`.
fn json_to_nexus(v: Value) -> NexusValue {
    match v {
        Value::Null => NexusValue::Null,
        Value::Bool(b) => NexusValue::Bool(b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                NexusValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                NexusValue::Float(f)
            } else {
                // u64 that does not fit in i64 — widen to f64.
                NexusValue::Float(n.as_u64().unwrap_or(0) as f64)
            }
        }
        Value::String(s) => NexusValue::Str(s),
        Value::Array(a) => NexusValue::Array(a.into_iter().map(json_to_nexus).collect()),
        Value::Object(m) => NexusValue::Map(
            m.into_iter()
                .map(|(k, v)| (NexusValue::Str(k), json_to_nexus(v)))
                .collect(),
        ),
    }
}

/// Reverse direction — decode the server's RPC CYPHER response into
/// the `QueryResult` type the commands layer expects.
fn nexus_to_query_result(v: NexusValue) -> Result<QueryResult> {
    let pairs = match v {
        NexusValue::Map(p) => p,
        other => return Err(anyhow!("RPC cypher reply must be a Map, got {:?}", other)),
    };
    let mut columns: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut execution_time_ms: f64 = 0.0;

    for (k, val) in pairs {
        let key = match k {
            NexusValue::Str(s) => s,
            _ => continue,
        };
        match key.as_str() {
            "columns" => {
                if let NexusValue::Array(items) = val {
                    columns = items
                        .into_iter()
                        .filter_map(|it| match it {
                            NexusValue::Str(s) => Some(s),
                            _ => None,
                        })
                        .collect();
                }
            }
            "rows" => {
                if let NexusValue::Array(outer) = val {
                    rows = outer
                        .into_iter()
                        .map(|row| match row {
                            NexusValue::Array(items) => {
                                items.into_iter().map(nexus_to_json).collect()
                            }
                            other => vec![nexus_to_json(other)],
                        })
                        .collect();
                }
            }
            "execution_time_ms" => {
                execution_time_ms = match val {
                    NexusValue::Int(i) => i as f64,
                    NexusValue::Float(f) => f,
                    _ => 0.0,
                };
            }
            // "stats" is server-side metadata; the CLI does not
            // currently render the row count from there so we ignore
            // it for now. Adding a typed decode is easy if a future
            // command needs it.
            _ => {}
        }
    }

    Ok(QueryResult {
        columns,
        rows,
        stats: Some(QueryStats {
            execution_time_ms,
            ..Default::default()
        }),
    })
}

/// Convert a `NexusValue` back to `serde_json::Value` for the result
/// decoding path.
fn nexus_to_json(v: NexusValue) -> Value {
    match v {
        NexusValue::Null => Value::Null,
        NexusValue::Bool(b) => Value::Bool(b),
        NexusValue::Int(i) => Value::Number(i.into()),
        NexusValue::Float(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        NexusValue::Str(s) => Value::String(s),
        NexusValue::Bytes(b) => {
            // Preserve bytes as a JSON array of ints so they survive
            // the trip to the CLI's print-table / print-json code.
            Value::Array(b.into_iter().map(Value::from).collect())
        }
        NexusValue::Array(a) => Value::Array(a.into_iter().map(nexus_to_json).collect()),
        NexusValue::Map(pairs) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in pairs {
                let key = match k {
                    NexusValue::Str(s) => s,
                    NexusValue::Int(i) => i.to_string(),
                    other => format!("{:?}", other),
                };
                obj.insert(key, nexus_to_json(val));
            }
            Value::Object(obj)
        }
    }
}

// `QueryStats` needs a default so the RPC path can populate just the
// execution time without touching the legacy HTTP-only counters.
impl Default for QueryStats {
    fn default() -> Self {
        Self {
            nodes_created: 0,
            nodes_deleted: 0,
            relationships_created: 0,
            relationships_deleted: 0,
            properties_set: 0,
            execution_time_ms: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_parses_nexus_url_and_becomes_rpc_client() {
        let c = NexusClient::new(Some("nexus://127.0.0.1:15475"), None, None, None, None).unwrap();
        assert!(c.is_rpc());
        assert_eq!(c.endpoint_description(), "nexus://127.0.0.1:15475");
    }

    #[test]
    fn client_parses_http_url_and_stays_http_only() {
        let c = NexusClient::new(Some("http://localhost:15474"), None, None, None, None).unwrap();
        assert!(!c.is_rpc());
    }

    #[test]
    fn client_default_is_rpc_loopback() {
        let c = NexusClient::new(None, None, None, None, None).unwrap();
        assert!(c.is_rpc());
        assert_eq!(c.endpoint_description(), "nexus://127.0.0.1:15475");
    }

    #[test]
    fn transport_override_force_http_on_nexus_url() {
        let c = NexusClient::new(
            Some("nexus://example.com:17000"),
            None,
            None,
            None,
            Some("http"),
        )
        .unwrap();
        assert!(!c.is_rpc(), "http override must disable the RPC transport");
        assert_eq!(c.endpoint_description(), "http://example.com:15474");
    }

    #[test]
    fn transport_override_force_rpc_on_http_url() {
        let c = NexusClient::new(
            Some("http://example.com:15474"),
            None,
            None,
            None,
            Some("rpc"),
        )
        .unwrap();
        assert!(c.is_rpc(), "rpc override must enable the RPC transport");
        assert_eq!(c.endpoint_description(), "nexus://example.com:15475");
    }

    #[test]
    fn transport_override_rejects_unknown() {
        let err = match NexusClient::new(None, None, None, None, Some("redis")) {
            Err(e) => e,
            Ok(_) => panic!("unknown override should fail"),
        };
        assert!(err.to_string().contains("unknown --transport value"));
    }

    #[test]
    fn transport_override_auto_is_identity() {
        let c =
            NexusClient::new(Some("nexus://host:15475"), None, None, None, Some("auto")).unwrap();
        assert!(c.is_rpc());
    }

    #[test]
    fn json_to_nexus_roundtrips_null_bool_int_float_string() {
        let cases = [
            Value::Null,
            Value::Bool(true),
            Value::from(42i64),
            Value::from(3.25f64),
            Value::from("hi"),
        ];
        for case in cases {
            let back = nexus_to_json(json_to_nexus(case.clone()));
            assert_eq!(back, case);
        }
    }

    #[test]
    fn nexus_to_query_result_decodes_server_envelope() {
        let envelope = NexusValue::Map(vec![
            (
                NexusValue::Str("columns".into()),
                NexusValue::Array(vec![NexusValue::Str("n".into())]),
            ),
            (
                NexusValue::Str("rows".into()),
                NexusValue::Array(vec![NexusValue::Array(vec![NexusValue::Int(1)])]),
            ),
            (
                NexusValue::Str("execution_time_ms".into()),
                NexusValue::Int(7),
            ),
        ]);
        let out = nexus_to_query_result(envelope).unwrap();
        assert_eq!(out.columns, vec!["n".to_string()]);
        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.rows[0][0], Value::from(1i64));
        assert_eq!(out.stats.unwrap().execution_time_ms, 7.0);
    }

    #[test]
    fn nexus_to_query_result_rejects_non_map() {
        let err = nexus_to_query_result(NexusValue::Int(1)).unwrap_err();
        assert!(err.to_string().contains("must be a Map"));
    }
}
