//! Nexus native RPC bench client. Feature-gated on `live-bench`.
//!
//! Talks to a running Nexus server over the length-prefixed
//! MessagePack wire protocol defined in [`nexus_protocol::rpc`].
//! Picked over HTTP/JSON on purpose: comparative runs put this
//! side next to the Neo4j Bolt client, and both transports are
//! binary, so the numbers reflect engine work rather than
//! serialisation overhead.
//!
//! Guard rails:
//!
//! * Every call wraps `tokio::time::timeout(timeout, ...)` so a
//!   hung server cannot wedge the harness runtime.
//! * `connect` performs a bounded `HELLO 1` + optional `AUTH`
//!   handshake, then a `PING` probe within 2 s — the same upper
//!   bound [`super::Neo4jBoltClient`] enforces on its `RETURN 1`
//!   health probe.
//! * Requests carry a monotonic id; responses whose id does not
//!   match are surfaced as [`ClientError::BadResponse`] so a
//!   duplicated or reordered frame cannot silently corrupt the
//!   reported latencies.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use nexus_protocol::rpc::PUSH_ID;
use nexus_protocol::rpc::codec::{read_response, write_request};
use nexus_protocol::rpc::types::{NexusValue, Request};
use tokio::io::BufReader;
use tokio::net::TcpStream;
use tokio::runtime::Handle;

use super::{BenchClient, ClientError, ExecOutcome, Row};

/// Authentication configuration for the Nexus RPC handshake.
/// An `AUTH` command is only issued when at least one of the
/// (api_key) or (username + password) triples is populated — a
/// server with auth disabled accepts connections without it.
#[derive(Debug, Clone, Default)]
pub struct NexusRpcCredentials {
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl NexusRpcCredentials {
    fn has_any(&self) -> bool {
        self.api_key.is_some() || (self.username.is_some() && self.password.is_some())
    }

    fn to_auth_args(&self) -> Vec<NexusValue> {
        if let Some(key) = &self.api_key {
            vec![NexusValue::Str(key.clone())]
        } else {
            vec![
                NexusValue::Str(self.username.clone().unwrap_or_default()),
                NexusValue::Str(self.password.clone().unwrap_or_default()),
            ]
        }
    }
}

/// Bench client that speaks the native Nexus RPC wire.
pub struct NexusRpcClient {
    stream: BufReader<TcpStream>,
    engine_label: String,
    runtime: Handle,
    next_id: AtomicU32,
}

impl NexusRpcClient {
    /// Connect to a running Nexus RPC listener. Performs `HELLO 1`,
    /// an optional `AUTH`, and a `PING` probe — all inside a 5 s
    /// total budget — before returning. A server that is not
    /// answering the RPC port within that window fails fast rather
    /// than letting a hung connect silently extend the first
    /// benchmark iteration.
    pub async fn connect(
        addr: impl AsRef<str>,
        credentials: NexusRpcCredentials,
        engine_label: impl Into<String>,
        runtime: Handle,
    ) -> Result<Self, ClientError> {
        let addr = addr.as_ref();
        let engine_label = engine_label.into();

        let open_fut = async {
            let stream = TcpStream::connect(addr)
                .await
                .map_err(|e| ClientError::Transport(format!("connect {addr}: {e}")))?;
            stream
                .set_nodelay(true)
                .map_err(|e| ClientError::Transport(format!("TCP_NODELAY: {e}")))?;
            Ok::<_, ClientError>(BufReader::new(stream))
        };
        let mut buf = tokio::time::timeout(Duration::from_secs(5), open_fut)
            .await
            .map_err(|_| ClientError::HealthProbe("RPC connect timed out after 5 s".into()))??;

        // HELLO 1 — pre-auth, always required.
        let hello = Request {
            id: 0,
            command: "HELLO".into(),
            args: vec![NexusValue::Int(1)],
        };
        send_and_recv(&mut buf, &hello, Duration::from_secs(2))
            .await
            .map_err(|e| ClientError::HealthProbe(format!("HELLO failed: {e}")))?;

        // AUTH only when the caller supplied credentials — an
        // unauthenticated server accepts any connection pre-auth.
        if credentials.has_any() {
            let auth = Request {
                id: 0,
                command: "AUTH".into(),
                args: credentials.to_auth_args(),
            };
            send_and_recv(&mut buf, &auth, Duration::from_secs(2))
                .await
                .map_err(|e| ClientError::HealthProbe(format!("AUTH failed: {e}")))?;
        }

        // PING — the RPC counterpart to the HTTP client's /health
        // probe. 2 s cap matches Neo4jBoltClient's RETURN 1 window.
        let ping = Request {
            id: 0,
            command: "PING".into(),
            args: vec![],
        };
        send_and_recv(&mut buf, &ping, Duration::from_secs(2))
            .await
            .map_err(|e| ClientError::HealthProbe(format!("PING failed: {e}")))?;

        Ok(Self {
            stream: buf,
            engine_label,
            runtime,
            next_id: AtomicU32::new(1),
        })
    }

    /// Runtime handle used to bridge the sync [`BenchClient`]
    /// contract into async tokio I/O.
    pub fn runtime(&self) -> &Handle {
        &self.runtime
    }

    /// Shared execute path — called from the sync [`BenchClient`]
    /// impl. Bounded by `timeout` end-to-end.
    async fn execute_async(
        &mut self,
        cypher: &str,
        timeout: Duration,
    ) -> Result<ExecOutcome, ClientError> {
        let id = self.reserve_id();
        let req = Request {
            id,
            command: "CYPHER".into(),
            args: vec![NexusValue::Str(cypher.to_string())],
        };
        let envelope = send_and_recv(&mut self.stream, &req, timeout).await?;
        rows_from_envelope(envelope).map(|rows| ExecOutcome { rows })
    }

    /// Reserve a request id, skipping the reserved server-push id.
    fn reserve_id(&self) -> u32 {
        let mut id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if id == PUSH_ID {
            id = self.next_id.fetch_add(1, Ordering::Relaxed);
        }
        id
    }
}

impl BenchClient for NexusRpcClient {
    fn engine_name(&self) -> &str {
        &self.engine_label
    }

    fn execute(&mut self, cypher: &str, timeout: Duration) -> Result<ExecOutcome, ClientError> {
        let cypher = cypher.to_string();
        let runtime = self.runtime.clone();
        tokio::task::block_in_place(|| {
            runtime.block_on(async { self.execute_async(&cypher, timeout).await })
        })
    }
}

/// Write `req`, read the matching response, enforce a bounded
/// timeout for the round-trip, and validate the response id.
async fn send_and_recv(
    stream: &mut BufReader<TcpStream>,
    req: &Request,
    timeout: Duration,
) -> Result<NexusValue, ClientError> {
    let rtt = async {
        write_request(stream.get_mut(), req)
            .await
            .map_err(|e| ClientError::Transport(format!("write: {e}")))?;
        let resp = read_response(stream)
            .await
            .map_err(|e| ClientError::Transport(format!("read: {e}")))?;
        if req.id != 0 && resp.id != req.id {
            return Err(ClientError::BadResponse(format!(
                "id mismatch: sent {}, received {}",
                req.id, resp.id
            )));
        }
        resp.result
            .map_err(|e| ClientError::BadResponse(format!("server: {e}")))
    };
    tokio::time::timeout(timeout, rtt)
        .await
        .map_err(|_| ClientError::Timeout(timeout))?
}

/// Convert the server's `CYPHER` response envelope — a
/// `Map { columns, rows, stats, execution_time_ms }` per the
/// server's RPC dispatch — into the neutral `Vec<Row>` the harness
/// consumes. Only the `rows` field is load-bearing here; the rest
/// is ignored.
fn rows_from_envelope(value: NexusValue) -> Result<Vec<Row>, ClientError> {
    let pairs = match value {
        NexusValue::Map(pairs) => pairs,
        other => {
            return Err(ClientError::BadResponse(format!(
                "CYPHER response was not a Map, got {:?}",
                std::mem::discriminant(&other)
            )));
        }
    };
    let mut rows_field: Option<NexusValue> = None;
    for (k, v) in pairs {
        if k.as_str() == Some("rows") {
            rows_field = Some(v);
            break;
        }
    }
    let rows = rows_field.ok_or_else(|| {
        ClientError::BadResponse("CYPHER response Map missing `rows` field".into())
    })?;
    let row_arrays = match rows {
        NexusValue::Array(xs) => xs,
        other => {
            return Err(ClientError::BadResponse(format!(
                "CYPHER `rows` was not an Array, got {:?}",
                std::mem::discriminant(&other)
            )));
        }
    };
    row_arrays
        .into_iter()
        .map(row_from_nexus)
        .collect::<Result<Vec<_>, _>>()
}

/// Convert a single row — a `NexusValue::Array` of cells — into
/// `Vec<serde_json::Value>`.
fn row_from_nexus(row: NexusValue) -> Result<Row, ClientError> {
    let cells = match row {
        NexusValue::Array(cells) => cells,
        other => {
            return Err(ClientError::BadResponse(format!(
                "row was not an Array, got {:?}",
                std::mem::discriminant(&other)
            )));
        }
    };
    cells.into_iter().map(nexus_to_json).collect()
}

/// Minimal `NexusValue -> serde_json::Value` converter — the
/// bench only needs to cross the boundary once per cell, and a
/// full-width converter lives in `nexus-server::protocol::rpc`.
/// Duplicated here to keep the bench's dependency graph narrow
/// (no `nexus-core`, no `nexus-server`).
fn nexus_to_json(value: NexusValue) -> Result<serde_json::Value, ClientError> {
    match value {
        NexusValue::Null => Ok(serde_json::Value::Null),
        NexusValue::Bool(b) => Ok(serde_json::Value::Bool(b)),
        NexusValue::Int(i) => Ok(serde_json::Value::Number(i.into())),
        NexusValue::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .ok_or_else(|| {
                ClientError::BadResponse("non-finite Float cannot be represented in JSON".into())
            }),
        NexusValue::Str(s) => Ok(serde_json::Value::String(s)),
        NexusValue::Bytes(b) => String::from_utf8(b)
            .map(serde_json::Value::String)
            .map_err(|_| ClientError::BadResponse("Bytes value must be valid UTF-8".into())),
        NexusValue::Array(items) => items
            .into_iter()
            .map(nexus_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        NexusValue::Map(pairs) => {
            let mut map = serde_json::Map::with_capacity(pairs.len());
            for (k, v) in pairs {
                let key = k
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| ClientError::BadResponse("map keys must be strings".into()))?;
                map.insert(key, nexus_to_json(v)?);
            }
            Ok(serde_json::Value::Object(map))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rpc_client_is_send_sync_benchclient() {
        fn assert_traits<T: BenchClient + Send + Sync + 'static>() {}
        assert_traits::<NexusRpcClient>();
    }

    #[test]
    fn credentials_has_any_recognises_full_combinations() {
        assert!(!NexusRpcCredentials::default().has_any());
        assert!(
            NexusRpcCredentials {
                api_key: Some("k".into()),
                ..Default::default()
            }
            .has_any()
        );
        assert!(
            !NexusRpcCredentials {
                username: Some("u".into()),
                ..Default::default()
            }
            .has_any(),
            "username alone is not credentials"
        );
        assert!(
            NexusRpcCredentials {
                username: Some("u".into()),
                password: Some("p".into()),
                ..Default::default()
            }
            .has_any()
        );
    }

    #[test]
    fn nexus_to_json_primitives() {
        assert_eq!(nexus_to_json(NexusValue::Null).unwrap(), json!(null));
        assert_eq!(nexus_to_json(NexusValue::Bool(true)).unwrap(), json!(true));
        assert_eq!(nexus_to_json(NexusValue::Int(42)).unwrap(), json!(42));
        assert_eq!(
            nexus_to_json(NexusValue::Float(0.5)).unwrap(),
            json!(0.5_f64)
        );
        assert_eq!(
            nexus_to_json(NexusValue::Str("hi".into())).unwrap(),
            json!("hi")
        );
    }

    #[test]
    fn nexus_to_json_rejects_non_finite_float() {
        let err = nexus_to_json(NexusValue::Float(f64::NAN)).unwrap_err();
        assert!(matches!(err, ClientError::BadResponse(_)));
    }

    #[test]
    fn nexus_to_json_nested_array_and_map() {
        let nested = NexusValue::Array(vec![
            NexusValue::Int(1),
            NexusValue::Map(vec![(NexusValue::Str("k".into()), NexusValue::Bool(true))]),
        ]);
        let out = nexus_to_json(nested).unwrap();
        assert_eq!(out, json!([1, { "k": true }]));
    }

    #[test]
    fn rows_from_envelope_extracts_rows_field() {
        let envelope = NexusValue::Map(vec![
            (
                NexusValue::Str("columns".into()),
                NexusValue::Array(vec![NexusValue::Str("n".into())]),
            ),
            (
                NexusValue::Str("rows".into()),
                NexusValue::Array(vec![
                    NexusValue::Array(vec![NexusValue::Int(1)]),
                    NexusValue::Array(vec![NexusValue::Int(2)]),
                ]),
            ),
            (
                NexusValue::Str("execution_time_ms".into()),
                NexusValue::Int(3),
            ),
        ]);
        let rows = rows_from_envelope(envelope).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec![json!(1)]);
        assert_eq!(rows[1], vec![json!(2)]);
    }

    #[test]
    fn rows_from_envelope_errors_when_rows_missing() {
        let envelope = NexusValue::Map(vec![(
            NexusValue::Str("columns".into()),
            NexusValue::Array(vec![]),
        )]);
        let err = rows_from_envelope(envelope).unwrap_err();
        assert!(matches!(err, ClientError::BadResponse(_)));
    }

    #[test]
    fn rows_from_envelope_errors_when_rows_is_not_array() {
        let envelope = NexusValue::Map(vec![(
            NexusValue::Str("rows".into()),
            NexusValue::Str("not an array".into()),
        )]);
        let err = rows_from_envelope(envelope).unwrap_err();
        assert!(matches!(err, ClientError::BadResponse(_)));
    }
}
