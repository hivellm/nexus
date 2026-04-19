//! HTTP fallback transport.
//!
//! Wraps `reqwest::Client` behind the same [`Transport`] interface
//! the RPC path uses, so `NexusClient` can pick a transport once at
//! construction and stop branching on scheme for every method call.
//!
//! The translation from wire-level command names to HTTP routes is a
//! thin hard-coded table — every HTTP route the legacy CLI used has a
//! mapping here. Commands without a mapping surface as a structured
//! error so callers see exactly why the fallback path refused the
//! request.

use crate::error::{NexusError, Result};
use async_trait::async_trait;
use base64::Engine;
use nexus_protocol::rpc::types::NexusValue;
use reqwest::{Client, Method};
use serde_json::Value;

use super::endpoint::{Endpoint, Scheme};
use super::{Transport, TransportRequest, TransportResponse};

/// HTTP credentials — API key or basic auth. Both may be set; the key
/// takes precedence to match the CLI's behaviour.
#[derive(Debug, Clone, Default)]
pub struct HttpCredentials {
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// HTTP transport over an existing `reqwest::Client`. Uses the
/// endpoint's `as_http_url()` method so `nexus://` URLs still get a
/// reachable HTTP URL (swapping to the sibling port 15474).
pub struct HttpTransport {
    endpoint: Endpoint,
    client: Client,
    base_url: String,
    credentials: HttpCredentials,
}

impl HttpTransport {
    pub fn new(
        endpoint: Endpoint,
        credentials: HttpCredentials,
        timeout_secs: u64,
    ) -> Result<Self> {
        let base_url = endpoint.as_http_url();
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .user_agent(format!("nexus-sdk/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| NexusError::Configuration(format!("reqwest build: {e}")))?;
        Ok(Self {
            endpoint,
            client,
            base_url,
            credentials,
        })
    }

    fn auth(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(key) = &self.credentials.api_key {
            req = req.header("X-API-Key", key);
        } else if let (Some(u), Some(p)) = (&self.credentials.username, &self.credentials.password)
        {
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"));
            req = req.header("Authorization", format!("Basic {token}"));
        }
        req
    }

    async fn dispatch(&self, cmd: &str, args: &[NexusValue]) -> Result<Value> {
        match cmd {
            "CYPHER" => {
                let query = first_str(args).ok_or_else(|| arg_err("CYPHER", 0, "string"))?;
                let params = args.get(1).map(nexus_to_json).unwrap_or(Value::Null);
                let body = serde_json::json!({
                    "query": query,
                    "parameters": if params.is_null() { Value::Null } else { params },
                });
                let url = format!("{}/cypher", self.base_url);
                let resp = self
                    .auth(self.client.request(Method::POST, &url).json(&body))
                    .send()
                    .await
                    .map_err(NexusError::Http)?;
                http_json(resp).await
            }
            "PING" | "HEALTH" => {
                let url = format!("{}/health", self.base_url);
                let resp = self
                    .auth(self.client.request(Method::GET, &url))
                    .send()
                    .await
                    .map_err(NexusError::Http)?;
                http_json(resp).await
            }
            "STATS" => {
                let url = format!("{}/stats", self.base_url);
                let resp = self
                    .auth(self.client.request(Method::GET, &url))
                    .send()
                    .await
                    .map_err(NexusError::Http)?;
                http_json(resp).await
            }
            "EXPORT" => {
                let fmt = first_str(args).ok_or_else(|| arg_err("EXPORT", 0, "string"))?;
                let url = format!("{}/export?format={}", self.base_url, fmt);
                let resp = self
                    .auth(self.client.request(Method::GET, &url))
                    .send()
                    .await
                    .map_err(NexusError::Http)?;
                let text = resp.text().await.map_err(NexusError::Http)?;
                Ok(serde_json::json!({"format": fmt, "data": text}))
            }
            "IMPORT" => {
                let fmt = first_str(args).ok_or_else(|| arg_err("IMPORT", 0, "string"))?;
                let payload = args
                    .get(1)
                    .and_then(|v| v.as_str().map(String::from))
                    .ok_or_else(|| arg_err("IMPORT", 1, "string"))?;
                let url = format!("{}/import?format={}", self.base_url, fmt);
                let resp = self
                    .auth(self.client.request(Method::POST, &url).body(payload))
                    .send()
                    .await
                    .map_err(NexusError::Http)?;
                http_json(resp).await
            }
            other => Err(NexusError::Configuration(format!(
                "HTTP fallback does not know how to route '{other}' \
                 — add an entry to nexus-sdk/src/transport/http.rs::dispatch"
            ))),
        }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn execute(&self, req: TransportRequest) -> Result<TransportResponse> {
        let json = self.dispatch(&req.command, &req.args).await?;
        Ok(TransportResponse {
            value: json_to_nexus(json),
        })
    }

    fn describe(&self) -> String {
        match self.endpoint.scheme {
            Scheme::Https => format!("{} (HTTPS)", self.endpoint),
            _ => format!("{} (HTTP)", self.endpoint),
        }
    }

    fn is_rpc(&self) -> bool {
        false
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn first_str(args: &[NexusValue]) -> Option<String> {
    args.first().and_then(|v| v.as_str().map(String::from))
}

fn arg_err(cmd: &str, idx: usize, ty: &str) -> NexusError {
    NexusError::Configuration(format!(
        "HTTP fallback: '{cmd}' argument {idx} must be a {ty}"
    ))
}

async fn http_json(resp: reqwest::Response) -> Result<Value> {
    let status = resp.status();
    if status.is_success() {
        resp.json::<Value>().await.map_err(NexusError::Http)
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(NexusError::Api {
            message: body,
            status: status.as_u16(),
        })
    }
}

/// JSON → NexusValue (mirror of the CLI helper of the same name).
pub(crate) fn json_to_nexus(v: Value) -> NexusValue {
    match v {
        Value::Null => NexusValue::Null,
        Value::Bool(b) => NexusValue::Bool(b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                NexusValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                NexusValue::Float(f)
            } else {
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

/// NexusValue → JSON (mirror of the CLI helper).
pub(crate) fn nexus_to_json(v: &NexusValue) -> Value {
    match v {
        NexusValue::Null => Value::Null,
        NexusValue::Bool(b) => Value::Bool(*b),
        NexusValue::Int(i) => Value::Number((*i).into()),
        NexusValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        NexusValue::Str(s) => Value::String(s.clone()),
        NexusValue::Bytes(b) => Value::Array(b.iter().copied().map(Value::from).collect()),
        NexusValue::Array(a) => Value::Array(a.iter().map(nexus_to_json).collect()),
        NexusValue::Map(pairs) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in pairs {
                let key = match k {
                    NexusValue::Str(s) => s.clone(),
                    NexusValue::Int(i) => i.to_string(),
                    other => format!("{:?}", other),
                };
                obj.insert(key, nexus_to_json(val));
            }
            Value::Object(obj)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip_covers_primitive_variants() {
        let cases = [
            Value::Null,
            Value::Bool(true),
            Value::from(42i64),
            Value::from(3.25f64),
            Value::from("hi"),
        ];
        for case in cases {
            let back = nexus_to_json(&json_to_nexus(case.clone()));
            assert_eq!(back, case);
        }
    }

    #[test]
    fn json_roundtrip_handles_nested_map_and_array() {
        let src = serde_json::json!({
            "labels": ["Person"],
            "properties": {"name": "Alice", "age": 30}
        });
        let back = nexus_to_json(&json_to_nexus(src.clone()));
        assert_eq!(back, src);
    }

    #[tokio::test]
    async fn http_fallback_rejects_unknown_command() {
        let ep = Endpoint::parse("http://127.0.0.1:1").unwrap();
        let t = HttpTransport::new(ep, HttpCredentials::default(), 5).unwrap();
        let err = t
            .dispatch("WIDGET", &[])
            .await
            .expect_err("unknown cmd must error");
        assert!(format!("{err}").contains("does not know how to route"));
    }
}
