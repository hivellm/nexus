//! Thin RPC client used when the endpoint scheme is `nexus://`.
//!
//! The server lives in `nexus-server::protocol::rpc` and speaks the
//! length-prefixed MessagePack framing defined in
//! [`nexus_protocol::rpc`]. This module provides a minimal
//! request/response helper tailored to the CLI's needs:
//!
//! - Lazy connect on the first call.
//! - Optional `AUTH <api_key>` or `AUTH <username> <password>` on
//!   connect.
//! - Monotonic request ids per connection.
//! - `call(command, args)` returns the server's decoded [`NexusValue`]
//!   or a protocol error string.
//!
//! This client is **single-threaded-ish** — a `tokio::sync::Mutex`
//! serialises access to the underlying `TcpStream` so concurrent
//! callers never interleave frames. That is sufficient for the CLI
//! (which is strictly request/response) and keeps the implementation
//! tiny.

use anyhow::{Result, anyhow};
use nexus_protocol::rpc::codec::{read_response, write_request};
use nexus_protocol::rpc::types::{NexusValue, Request};
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::io::BufReader;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::endpoint::Endpoint;

/// Credentials used by [`RpcTransport::connect`]. The CLI supports two
/// authentication modes (API key or username/password) exactly like
/// the REST `/login` endpoint.
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

/// A lazily-connected RPC client over a single TCP stream.
pub struct RpcTransport {
    endpoint: Endpoint,
    credentials: RpcCredentials,
    /// `None` until the first call performs the connect+auth
    /// handshake. Wrapped in a `BufReader` for the response read path
    /// because `read_exact` on a plain `TcpStream` can split the
    /// 4-byte length prefix across multiple syscalls.
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

    /// Ensure the connection is open and authenticated, then issue a
    /// single request. Returns the server's decoded [`NexusValue`] or a
    /// bubbled-up error.
    pub async fn call(&self, command: &str, args: Vec<NexusValue>) -> Result<NexusValue> {
        let mut guard = self.stream.lock().await;
        if guard.is_none() {
            let s = Self::open(&self.endpoint, &self.credentials).await?;
            *guard = Some(s);
        }
        let stream = guard
            .as_mut()
            .expect("connection initialised above; guard holds Some");

        // Reserve an id. u32::MAX is reserved for server push, skip it.
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
            .map_err(|e| anyhow!("failed to send RPC frame: {}", e))?;

        let resp = read_response(stream)
            .await
            .map_err(|e| anyhow!("failed to read RPC frame: {}", e))?;
        if resp.id != id {
            return Err(anyhow!(
                "RPC id mismatch (expected {}, got {}) — connection is out of sync",
                id,
                resp.id
            ));
        }
        resp.result.map_err(|e| anyhow!("server: {}", e))
    }

    async fn open(
        endpoint: &Endpoint,
        credentials: &RpcCredentials,
    ) -> Result<BufReader<TcpStream>> {
        let authority = endpoint.authority();
        let stream = TcpStream::connect(&authority)
            .await
            .map_err(|e| anyhow!("failed to connect to {}: {}", authority, e))?;
        // Disable Nagle so tiny CLI request/response pairs don't eat
        // the 40ms ACK coalescing window.
        stream
            .set_nodelay(true)
            .map_err(|e| anyhow!("failed to set TCP_NODELAY: {}", e))?;
        let mut buf = BufReader::new(stream);

        // `HELLO 1` — proto-version handshake. Matches the server's
        // pre-auth allow-list and lets the client discover the active
        // protocol revision (currently always `1`).
        write_request(
            buf.get_mut(),
            &Request {
                id: 0,
                command: "HELLO".to_string(),
                args: vec![NexusValue::Int(1)],
            },
        )
        .await
        .map_err(|e| anyhow!("failed to send HELLO: {}", e))?;
        let hello = read_response(&mut buf)
            .await
            .map_err(|e| anyhow!("failed to read HELLO reply: {}", e))?;
        if let Err(e) = hello.result {
            return Err(anyhow!("HELLO rejected by server: {}", e));
        }

        // Authenticate if credentials were supplied. Servers that
        // disabled auth accept any connection pre-auth, so we only
        // issue AUTH when the user gave us something to send.
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
            .map_err(|e| anyhow!("failed to send AUTH: {}", e))?;
            let auth = read_response(&mut buf)
                .await
                .map_err(|e| anyhow!("failed to read AUTH reply: {}", e))?;
            if let Err(e) = auth.result {
                return Err(anyhow!("authentication failed: {}", e));
            }
        }

        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_any_needs_api_key_or_full_user_pass() {
        let empty = RpcCredentials::default();
        assert!(!empty.has_any());

        let key_only = RpcCredentials {
            api_key: Some("k".into()),
            ..Default::default()
        };
        assert!(key_only.has_any());

        let user_only = RpcCredentials {
            username: Some("u".into()),
            ..Default::default()
        };
        assert!(!user_only.has_any(), "username alone is not credentials");

        let full = RpcCredentials {
            username: Some("u".into()),
            password: Some("p".into()),
            ..Default::default()
        };
        assert!(full.has_any());
    }

    #[tokio::test]
    async fn call_fails_fast_when_server_is_unreachable() {
        let ep = Endpoint::parse("nexus://127.0.0.1:1").unwrap();
        let t = RpcTransport::new(ep, RpcCredentials::default());
        let err = t.call("PING", vec![]).await.unwrap_err();
        assert!(
            err.to_string().contains("failed to connect"),
            "error should name the connect step: {}",
            err
        );
    }
}
