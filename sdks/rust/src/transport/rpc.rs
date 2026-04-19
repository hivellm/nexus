//! Native binary RPC transport for the Rust SDK.
//!
//! Wraps the wire format defined in `nexus-protocol::rpc` in a
//! lazily-connected TCP client. Matches the CLI's `rpc_transport`
//! behaviour (HELLO handshake, optional AUTH, monotonic request ids,
//! `PUSH_ID` avoidance) but ships as part of the SDK rather than the
//! CLI binary.

use crate::error::{NexusError, Result};
use async_trait::async_trait;
use nexus_protocol::rpc::codec::{read_response, write_request};
use nexus_protocol::rpc::types::{NexusValue, Request};
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::io::BufReader;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use super::endpoint::Endpoint;
use super::{Transport, TransportRequest, TransportResponse};

#[derive(Debug, Clone, Default)]
pub struct RpcCredentials {
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl RpcCredentials {
    pub fn has_any(&self) -> bool {
        self.api_key.is_some() || (self.username.is_some() && self.password.is_some())
    }
}

/// Native binary RPC transport. Single TCP stream guarded by a
/// `tokio::sync::Mutex` so concurrent callers never interleave frames.
pub struct RpcTransport {
    endpoint: Endpoint,
    credentials: RpcCredentials,
    stream: Mutex<Option<BufReader<TcpStream>>>,
    next_id: AtomicU32,
}

impl RpcTransport {
    pub fn new(endpoint: Endpoint, credentials: RpcCredentials) -> Self {
        Self {
            endpoint,
            credentials,
            stream: Mutex::new(None),
            next_id: AtomicU32::new(1),
        }
    }

    /// Shortcut for `execute` that takes a raw command + arg vector
    /// without going through the `TransportRequest` wrapper. Handy
    /// for tests that want a single call site.
    pub async fn call(&self, command: &str, args: Vec<NexusValue>) -> Result<NexusValue> {
        let mut guard = self.stream.lock().await;
        if guard.is_none() {
            let s = Self::open(&self.endpoint, &self.credentials).await?;
            *guard = Some(s);
        }
        let stream = guard
            .as_mut()
            .expect("connection initialised above; guard holds Some");

        let mut id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if id == nexus_protocol::rpc::PUSH_ID {
            id = self.next_id.fetch_add(1, Ordering::Relaxed);
        }

        let req = Request {
            id,
            command: command.to_string(),
            args,
        };
        write_request(stream.get_mut(), &req)
            .await
            .map_err(|e| NexusError::Network(format!("failed to send RPC frame: {}", e)))?;

        let resp = read_response(stream)
            .await
            .map_err(|e| NexusError::Network(format!("failed to read RPC frame: {}", e)))?;
        if resp.id != id {
            return Err(NexusError::Network(format!(
                "RPC id mismatch (expected {}, got {}) — connection is out of sync",
                id, resp.id
            )));
        }
        resp.result.map_err(|e| NexusError::Api {
            message: format!("server: {}", e),
            status: 0,
        })
    }

    async fn open(
        endpoint: &Endpoint,
        credentials: &RpcCredentials,
    ) -> Result<BufReader<TcpStream>> {
        let authority = endpoint.authority();
        let stream = TcpStream::connect(&authority).await.map_err(|e| {
            NexusError::Network(format!("failed to connect to {}: {}", authority, e))
        })?;
        stream
            .set_nodelay(true)
            .map_err(|e| NexusError::Network(format!("failed to set TCP_NODELAY: {}", e)))?;
        let mut buf = BufReader::new(stream);

        // HELLO 1 — proto-version handshake.
        write_request(
            buf.get_mut(),
            &Request {
                id: 0,
                command: "HELLO".to_string(),
                args: vec![NexusValue::Int(1)],
            },
        )
        .await
        .map_err(|e| NexusError::Network(format!("failed to send HELLO: {}", e)))?;
        let hello = read_response(&mut buf)
            .await
            .map_err(|e| NexusError::Network(format!("failed to read HELLO reply: {}", e)))?;
        if let Err(e) = hello.result {
            return Err(NexusError::Api {
                message: format!("HELLO rejected by server: {}", e),
                status: 0,
            });
        }

        // Optional AUTH.
        if credentials.has_any() {
            let args = if let Some(key) = &credentials.api_key {
                vec![NexusValue::Str(key.clone())]
            } else {
                vec![
                    NexusValue::Str(credentials.username.clone().unwrap_or_default()),
                    NexusValue::Str(credentials.password.clone().unwrap_or_default()),
                ]
            };
            write_request(
                buf.get_mut(),
                &Request {
                    id: 0,
                    command: "AUTH".to_string(),
                    args,
                },
            )
            .await
            .map_err(|e| NexusError::Network(format!("failed to send AUTH: {}", e)))?;
            let auth = read_response(&mut buf)
                .await
                .map_err(|e| NexusError::Network(format!("failed to read AUTH reply: {}", e)))?;
            if let Err(e) = auth.result {
                return Err(NexusError::Api {
                    message: format!("authentication failed: {}", e),
                    status: 0,
                });
            }
        }

        Ok(buf)
    }
}

#[async_trait]
impl Transport for RpcTransport {
    async fn execute(&self, req: TransportRequest) -> Result<TransportResponse> {
        let value = self.call(&req.command, req.args).await?;
        Ok(TransportResponse { value })
    }

    fn describe(&self) -> String {
        format!("{} (RPC)", self.endpoint)
    }

    fn is_rpc(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_any_needs_api_key_or_full_user_pass() {
        assert!(!RpcCredentials::default().has_any());
        assert!(
            RpcCredentials {
                api_key: Some("k".into()),
                ..Default::default()
            }
            .has_any()
        );
        assert!(
            !RpcCredentials {
                username: Some("u".into()),
                ..Default::default()
            }
            .has_any()
        );
        assert!(
            RpcCredentials {
                username: Some("u".into()),
                password: Some("p".into()),
                ..Default::default()
            }
            .has_any()
        );
    }

    #[tokio::test]
    async fn call_fails_fast_when_server_is_unreachable() {
        let ep = Endpoint::parse("nexus://127.0.0.1:1").unwrap();
        let t = RpcTransport::new(ep, RpcCredentials::default());
        let err = t.call("PING", vec![]).await.unwrap_err();
        assert!(
            format!("{err}").contains("failed to connect"),
            "error should name connect failure: {err}"
        );
    }
}
