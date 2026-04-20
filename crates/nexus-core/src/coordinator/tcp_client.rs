//! TCP-backed [`super::scatter::ShardClient`].
//!
//! The coordinator's [`super::scatter::ScatterGather`] engine calls
//! `execute(shard, cypher, params, gen, deadline)` once per targeted
//! shard; this module turns each such call into a TCP round-trip to the
//! shard's Raft leader:
//!
//! 1. Resolve the shard → leader via [`LeaderCache`] (falling back to
//!    `node_addrs` if nothing cached).
//! 2. Dial the leader with the `connect_timeout`.
//! 3. Write a single [`ShardRpcRequest`] frame, read a single
//!    [`ShardRpcResponse`] frame, close.
//! 4. On `NotLeader { leader_hint }` reply, update the cache so the
//!    next attempt (the coordinator retries up to 3×) lands on the
//!    new leader.
//!
//! The trait [`ShardClient::execute`] is **synchronous**. This module
//! bridges to async with `tokio::task::block_in_place` +
//! `Handle::block_on`, which means the caller MUST be inside a
//! multi-threaded tokio runtime (the nexus-server runtime, or
//! `#[tokio::test(flavor = "multi_thread")]` in tests). Calling from a
//! current-thread runtime panics — that's a loud failure we want, not
//! a silent deadlock.
//!
//! # Not for hot-path multiplexing
//!
//! One TCP connection per request is simple, testable, and
//! sufficient for the initial V2 rollout (dozens to hundreds of
//! shard RPCs per second). A connection-pooled multiplexing variant
//! is a performance follow-up — the Phase 4 benchmark suite will
//! decide when it's worth it. Dropping in the optimization doesn't
//! change the [`ShardClient`] trait.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::runtime::Handle;

use super::scatter::{ShardClient, ShardResponse};
use crate::sharding::metadata::{NodeId, ShardId};

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// Request sent coordinator → shard leader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardRpcRequest {
    /// Monotonic per-client request id. Included so a
    /// multiplexing variant can be added without breaking the wire
    /// format.
    pub request_id: u64,
    /// Shard the request is addressed to. Leaders MUST validate this
    /// against their own shard assignment.
    pub shard_id: ShardId,
    /// Coordinator's cached cluster generation. Shards reject stale
    /// values with [`ShardResponse::StaleGeneration`].
    pub generation: u64,
    /// Cypher source text the shard executes against its local state.
    pub cypher: String,
    /// Parameter bindings passed through unchanged.
    pub parameters: serde_json::Map<String, serde_json::Value>,
}

/// Response sent shard leader → coordinator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShardRpcResponse {
    /// Matches the request's `request_id`.
    pub request_id: u64,
    /// Payload the scatter engine consumes directly.
    pub payload: ShardResponse,
}

/// Wire-frame type byte. Distinct from Raft's `0x40` so a
/// misdirected Raft frame on this port is rejected cleanly.
pub const SHARD_RPC_REQUEST: u8 = 0x60;
/// Response type byte.
pub const SHARD_RPC_RESPONSE: u8 = 0x61;

const HEADER_LEN: usize = 9;
const CRC_LEN: usize = 4;
const MAX_PAYLOAD: usize = 64 * 1024 * 1024;

/// Errors produced while moving bytes over the socket.
#[derive(Debug, thiserror::Error)]
pub enum RpcIoError {
    /// Underlying I/O failed.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Bincode refused to (de)serialize.
    #[error("bincode: {0}")]
    Bincode(String),
    /// Frame header corrupted / wrong type / too large.
    #[error("bad frame: {0}")]
    BadFrame(String),
    /// CRC mismatch on read.
    #[error("CRC mismatch")]
    CrcMismatch,
    /// Deadline elapsed before the RPC completed.
    #[error("deadline elapsed")]
    Timeout,
}

fn encode<T: Serialize>(kind: u8, shard: ShardId, value: &T) -> Result<Vec<u8>, RpcIoError> {
    // MessagePack rather than bincode — the request carries a
    // serde_json::Value map (via `parameters`) and the response wraps
    // rows of Value, both of which trigger serde's `deserialize_any`.
    // Bincode 1.x's externally-tagged wire format does not implement
    // that path; rmp-serde does.
    let payload = rmp_serde::to_vec_named(value).map_err(|e| RpcIoError::Bincode(e.to_string()))?;
    if payload.len() > MAX_PAYLOAD {
        return Err(RpcIoError::BadFrame(format!(
            "payload {} bytes exceeds {MAX_PAYLOAD}",
            payload.len()
        )));
    }
    let mut buf = Vec::with_capacity(HEADER_LEN + payload.len() + CRC_LEN);
    buf.extend_from_slice(&shard.as_u32().to_le_bytes());
    buf.push(kind);
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(&payload);
    let mut h = crc32fast::Hasher::new();
    h.update(&buf);
    buf.extend_from_slice(&h.finalize().to_le_bytes());
    Ok(buf)
}

async fn read_frame<R, T>(reader: &mut R, expected_kind: u8) -> Result<(ShardId, T), RpcIoError>
where
    R: tokio::io::AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut header = [0u8; HEADER_LEN];
    reader.read_exact(&mut header).await?;
    let shard = ShardId::new(u32::from_le_bytes([
        header[0], header[1], header[2], header[3],
    ]));
    let kind = header[4];
    if kind != expected_kind {
        return Err(RpcIoError::BadFrame(format!(
            "expected type 0x{expected_kind:02x}, got 0x{kind:02x}"
        )));
    }
    let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
    if len > MAX_PAYLOAD {
        return Err(RpcIoError::BadFrame(format!(
            "declared length {len} exceeds {MAX_PAYLOAD}"
        )));
    }
    let mut rest = vec![0u8; len + CRC_LEN];
    reader.read_exact(&mut rest).await?;
    let (payload, crc_bytes) = rest.split_at(len);
    let expected_crc = u32::from_le_bytes([crc_bytes[0], crc_bytes[1], crc_bytes[2], crc_bytes[3]]);
    let mut h = crc32fast::Hasher::new();
    h.update(&header);
    h.update(payload);
    if h.finalize() != expected_crc {
        return Err(RpcIoError::CrcMismatch);
    }
    let value: T =
        rmp_serde::from_slice(payload).map_err(|e| RpcIoError::Bincode(e.to_string()))?;
    Ok((shard, value))
}

// Pub(crate) helper — the Phase 3 shard-side server reads requests
// via this function; here we expose it so its unit test can verify
// round-trips without opening a socket.
pub(crate) async fn write_request<W>(
    writer: &mut W,
    req: &ShardRpcRequest,
) -> Result<(), RpcIoError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let buf = encode(SHARD_RPC_REQUEST, req.shard_id, req)?;
    writer.write_all(&buf).await?;
    writer.flush().await?;
    Ok(())
}

pub(crate) async fn read_request<R>(reader: &mut R) -> Result<ShardRpcRequest, RpcIoError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let (_hdr_shard, req): (_, ShardRpcRequest) = read_frame(reader, SHARD_RPC_REQUEST).await?;
    Ok(req)
}

pub(crate) async fn write_response<W>(
    writer: &mut W,
    shard: ShardId,
    resp: &ShardRpcResponse,
) -> Result<(), RpcIoError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let buf = encode(SHARD_RPC_RESPONSE, shard, resp)?;
    writer.write_all(&buf).await?;
    writer.flush().await?;
    Ok(())
}

async fn read_response<R>(reader: &mut R) -> Result<ShardRpcResponse, RpcIoError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let (_hdr_shard, resp): (_, ShardRpcResponse) = read_frame(reader, SHARD_RPC_RESPONSE).await?;
    Ok(resp)
}

// ---------------------------------------------------------------------------
// Leader cache + address table
// ---------------------------------------------------------------------------

/// Tracks which node currently serves each shard. Populated lazily from
/// cluster metadata and from `NotLeader` hints returned by shard RPCs.
#[derive(Debug, Default)]
pub struct LeaderCache {
    inner: Mutex<BTreeMap<ShardId, NodeId>>,
}

impl LeaderCache {
    /// Empty cache. Lookups return `None` until populated.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set `node` as the leader for `shard`. Idempotent.
    pub fn update(&self, shard: ShardId, node: NodeId) {
        self.inner.lock().insert(shard, node);
    }

    /// Clear the cached entry — called when a leader is confirmed
    /// dead so the next RPC falls through to a random replica.
    pub fn invalidate(&self, shard: ShardId) {
        self.inner.lock().remove(&shard);
    }

    /// Current cached leader, if any.
    #[must_use]
    pub fn get(&self, shard: ShardId) -> Option<NodeId> {
        self.inner.lock().get(&shard).cloned()
    }
}

// ---------------------------------------------------------------------------
// TcpShardClient
// ---------------------------------------------------------------------------

/// Tunables.
#[derive(Debug, Clone)]
pub struct TcpShardClientConfig {
    /// TCP connect timeout for each RPC.
    pub connect_timeout: Duration,
    /// Per-RPC wire deadline safety net on top of the scatter-level
    /// `deadline`. Must be ≤ scatter timeout.
    pub rpc_timeout: Duration,
}

impl Default for TcpShardClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(2),
            rpc_timeout: Duration::from_secs(30),
        }
    }
}

/// TCP implementation of [`ShardClient`].
pub struct TcpShardClient {
    cfg: TcpShardClientConfig,
    /// `node_id` → address table. Populated at construction and by the
    /// cluster-metadata refresh path.
    node_addrs: Mutex<BTreeMap<NodeId, SocketAddr>>,
    /// Per-shard leader cache.
    leader_cache: Arc<LeaderCache>,
    /// Fallback replica ordering per shard — used when the leader
    /// cache is empty or the leader RPC fails. Kept in insertion
    /// order so tests can assert deterministic fan-out.
    shard_members: Mutex<BTreeMap<ShardId, Vec<NodeId>>>,
    /// Tokio handle used to bridge sync `execute` → async dial/send.
    runtime: Handle,
    /// Monotonic request-id counter.
    next_request_id: std::sync::atomic::AtomicU64,
}

impl TcpShardClient {
    /// Build a client bound to the given runtime. `node_addrs` maps
    /// every reachable node to its RPC listener address; `shard_members`
    /// lists the Raft membership per shard in preferred-order.
    #[must_use]
    pub fn new(
        cfg: TcpShardClientConfig,
        runtime: Handle,
        node_addrs: BTreeMap<NodeId, SocketAddr>,
        shard_members: BTreeMap<ShardId, Vec<NodeId>>,
        leader_cache: Arc<LeaderCache>,
    ) -> Self {
        Self {
            cfg,
            node_addrs: Mutex::new(node_addrs),
            leader_cache,
            shard_members: Mutex::new(shard_members),
            runtime,
            next_request_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Share the underlying leader cache. Useful for observability and
    /// for integration tests that want to assert cache state.
    #[must_use]
    pub fn leader_cache(&self) -> Arc<LeaderCache> {
        self.leader_cache.clone()
    }

    /// Update the node-address table (e.g. after a metadata refresh).
    pub fn set_node_addr(&self, node: NodeId, addr: SocketAddr) {
        self.node_addrs.lock().insert(node, addr);
    }

    /// Update the shard-membership table.
    pub fn set_shard_members(&self, shard: ShardId, members: Vec<NodeId>) {
        self.shard_members.lock().insert(shard, members);
    }

    fn candidates_for(&self, shard: ShardId) -> Vec<NodeId> {
        let mut seen: Vec<NodeId> = Vec::new();
        if let Some(l) = self.leader_cache.get(shard) {
            seen.push(l);
        }
        if let Some(members) = self.shard_members.lock().get(&shard) {
            for m in members {
                if !seen.iter().any(|n| n == m) {
                    seen.push(m.clone());
                }
            }
        }
        seen
    }

    fn addr_of(&self, node: &NodeId) -> Option<SocketAddr> {
        self.node_addrs.lock().get(node).copied()
    }

    async fn dispatch_once(
        &self,
        addr: SocketAddr,
        req: &ShardRpcRequest,
        deadline: Instant,
    ) -> Result<ShardResponse, RpcIoError> {
        let now = Instant::now();
        if now >= deadline {
            return Err(RpcIoError::Timeout);
        }
        let remaining = deadline - now;
        let connect_budget = remaining.min(self.cfg.connect_timeout);
        let mut stream = tokio::time::timeout(connect_budget, TcpStream::connect(addr))
            .await
            .map_err(|_| RpcIoError::Timeout)??;
        let _ = stream.set_nodelay(true);

        let write_budget = deadline.saturating_duration_since(Instant::now());
        if write_budget.is_zero() {
            return Err(RpcIoError::Timeout);
        }
        tokio::time::timeout(write_budget, write_request(&mut stream, req))
            .await
            .map_err(|_| RpcIoError::Timeout)??;

        let read_budget = deadline.saturating_duration_since(Instant::now());
        if read_budget.is_zero() {
            return Err(RpcIoError::Timeout);
        }
        let resp = tokio::time::timeout(read_budget, read_response(&mut stream))
            .await
            .map_err(|_| RpcIoError::Timeout)??;
        if resp.request_id != req.request_id {
            return Err(RpcIoError::BadFrame(format!(
                "response request_id={} but request was {}",
                resp.request_id, req.request_id
            )));
        }
        Ok(resp.payload)
    }

    async fn execute_async(
        &self,
        shard: ShardId,
        cypher: &str,
        parameters: &serde_json::Map<String, serde_json::Value>,
        generation: u64,
        deadline: Instant,
    ) -> ShardResponse {
        let request_id = self
            .next_request_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let req = ShardRpcRequest {
            request_id,
            shard_id: shard,
            generation,
            cypher: cypher.to_string(),
            parameters: parameters.clone(),
        };

        let candidates = self.candidates_for(shard);
        if candidates.is_empty() {
            return ShardResponse::ShardError {
                reason: format!("no known members for {shard}"),
            };
        }

        // Try candidates in order. The scatter engine does its own
        // leader-hint retry cycle on top of this (3 attempts per
        // RPC), but we also try each candidate once per call so a
        // single `execute` can fall through from a dead leader to a
        // live replica without needing the outer retry.
        let mut last_error: Option<String> = None;
        for node in &candidates {
            if Instant::now() >= deadline {
                return ShardResponse::ShardTimeout;
            }
            let addr = match self.addr_of(node) {
                Some(a) => a,
                None => {
                    last_error = Some(format!("no address for {node}"));
                    continue;
                }
            };
            match self.dispatch_once(addr, &req, deadline).await {
                Ok(payload) => {
                    // Keep the leader cache warm on success.
                    if let ShardResponse::Ok { .. } = &payload {
                        self.leader_cache.update(shard, node.clone());
                    }
                    if let ShardResponse::NotLeader { leader_hint } = &payload {
                        if let Some(hint) = leader_hint {
                            self.leader_cache.update(shard, hint.clone());
                        } else {
                            self.leader_cache.invalidate(shard);
                        }
                    }
                    return payload;
                }
                Err(RpcIoError::Timeout) => {
                    return ShardResponse::ShardTimeout;
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                    // Invalidate leader hint — this candidate is
                    // unreachable or returned a bad frame.
                    self.leader_cache.invalidate(shard);
                    continue;
                }
            }
        }
        ShardResponse::ShardError {
            reason: last_error.unwrap_or_else(|| format!("all candidates for {shard} unreachable")),
        }
    }
}

impl ShardClient for TcpShardClient {
    fn execute(
        &self,
        shard: ShardId,
        cypher: &str,
        parameters: &serde_json::Map<String, serde_json::Value>,
        generation: u64,
        deadline: Instant,
    ) -> ShardResponse {
        // Bridge sync ShardClient contract → async dispatch. Requires
        // a multi-threaded tokio runtime; the REST handler on
        // nexus-server uses `rt.block_on` on a multi-thread runtime
        // and the scatter engine runs inside `spawn_blocking` so this
        // is safe in production.
        let fut = self.execute_async(shard, cypher, parameters, generation, deadline);
        tokio::task::block_in_place(|| self.runtime.block_on(fut))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::scatter::ShardResponse;
    use serde_json::Value;
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;

    /// Minimal test server: accepts one request, calls `handler`, writes
    /// back the response, closes.
    async fn run_once<F>(listener: TcpListener, handler: F)
    where
        F: Fn(ShardRpcRequest) -> ShardResponse + Send + 'static,
    {
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                if let Ok(req) = read_request(&mut stream).await {
                    let payload = handler(req.clone());
                    let resp = ShardRpcResponse {
                        request_id: req.request_id,
                        payload,
                    };
                    let _ = write_response(&mut stream, req.shard_id, &resp).await;
                }
            }
        });
    }

    fn nid(s: &str) -> NodeId {
        NodeId::new(s).unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rpc_wire_roundtrip() {
        use tokio::io::duplex;
        let (mut a, mut b) = duplex(4096);
        let req = ShardRpcRequest {
            request_id: 42,
            shard_id: ShardId::new(3),
            generation: 5,
            cypher: "RETURN 1".into(),
            parameters: Default::default(),
        };
        let req2 = req.clone();
        tokio::spawn(async move {
            write_request(&mut a, &req2).await.unwrap();
        });
        let got = read_request(&mut b).await.unwrap();
        assert_eq!(got, req);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rpc_wire_rejects_wrong_type() {
        use tokio::io::{AsyncWriteExt, duplex};
        let (mut a, mut b) = duplex(4096);
        // Write a response frame and try to read it as a request.
        let resp = ShardRpcResponse {
            request_id: 1,
            payload: ShardResponse::Ok { rows: vec![] },
        };
        write_response(&mut a, ShardId::new(0), &resp)
            .await
            .unwrap();
        a.flush().await.unwrap();
        let err = read_request(&mut b).await.unwrap_err();
        assert!(matches!(err, RpcIoError::BadFrame(_)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn execute_round_trips_through_stub_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        run_once(listener, |req| {
            // Echo the request id back via a synthetic row.
            ShardResponse::Ok {
                rows: vec![vec![
                    Value::from(req.shard_id.as_u32()),
                    Value::from(req.request_id as i64),
                    Value::from(req.cypher),
                ]],
            }
        })
        .await;

        let mut addrs = BTreeMap::new();
        addrs.insert(nid("leader"), addr);
        let mut members = BTreeMap::new();
        members.insert(ShardId::new(0), vec![nid("leader")]);

        let client = TcpShardClient::new(
            TcpShardClientConfig::default(),
            Handle::current(),
            addrs,
            members,
            Arc::new(LeaderCache::new()),
        );

        // Execute is a sync call; block_in_place bridges to async.
        let out = tokio::task::spawn_blocking(move || {
            client.execute(
                ShardId::new(0),
                "MATCH (n) RETURN n",
                &serde_json::Map::new(),
                1,
                Instant::now() + Duration::from_secs(5),
            )
        })
        .await
        .unwrap();

        match out {
            ShardResponse::Ok { rows } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], Value::from(0));
                assert_eq!(rows[0][1], Value::from(1));
                assert_eq!(rows[0][2], Value::from("MATCH (n) RETURN n"));
            }
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn not_leader_updates_leader_cache() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        run_once(listener, |_req| ShardResponse::NotLeader {
            leader_hint: Some(nid("node-b")),
        })
        .await;

        let mut addrs = BTreeMap::new();
        addrs.insert(nid("node-a"), addr);
        let mut members = BTreeMap::new();
        members.insert(ShardId::new(0), vec![nid("node-a")]);
        let cache = Arc::new(LeaderCache::new());

        let client = TcpShardClient::new(
            TcpShardClientConfig::default(),
            Handle::current(),
            addrs,
            members,
            cache.clone(),
        );
        let cache_view = cache.clone();
        let resp = tokio::task::spawn_blocking(move || {
            client.execute(
                ShardId::new(0),
                "",
                &serde_json::Map::new(),
                1,
                Instant::now() + Duration::from_secs(2),
            )
        })
        .await
        .unwrap();
        assert!(matches!(
            resp,
            ShardResponse::NotLeader {
                leader_hint: Some(_)
            }
        ));
        assert_eq!(cache_view.get(ShardId::new(0)), Some(nid("node-b")));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn execute_without_known_members_errors() {
        let client = TcpShardClient::new(
            TcpShardClientConfig::default(),
            Handle::current(),
            BTreeMap::new(),
            BTreeMap::new(),
            Arc::new(LeaderCache::new()),
        );
        let resp = tokio::task::spawn_blocking(move || {
            client.execute(
                ShardId::new(0),
                "",
                &serde_json::Map::new(),
                1,
                Instant::now() + Duration::from_secs(1),
            )
        })
        .await
        .unwrap();
        assert!(matches!(resp, ShardResponse::ShardError { .. }));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn execute_past_deadline_returns_timeout() {
        let mut addrs = BTreeMap::new();
        addrs.insert(nid("dead"), "127.0.0.1:1".parse().unwrap());
        let mut members = BTreeMap::new();
        members.insert(ShardId::new(0), vec![nid("dead")]);
        let client = TcpShardClient::new(
            TcpShardClientConfig {
                connect_timeout: Duration::from_millis(50),
                ..Default::default()
            },
            Handle::current(),
            addrs,
            members,
            Arc::new(LeaderCache::new()),
        );
        let resp = tokio::task::spawn_blocking(move || {
            client.execute(
                ShardId::new(0),
                "",
                &serde_json::Map::new(),
                1,
                Instant::now() + Duration::from_millis(100),
            )
        })
        .await
        .unwrap();
        // Either Timeout (if connect timed out exactly) or ShardError
        // wrapping a connect-refused (if the port is free and the OS
        // rejects immediately). Both are legitimate deadline signals
        // — the coordinator translates Timeout → ERR_QUERY_TIMEOUT
        // and ShardError → ERR_SHARD_FAILURE, which the spec accepts.
        assert!(matches!(
            resp,
            ShardResponse::ShardTimeout | ShardResponse::ShardError { .. }
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn leader_cache_tried_first() {
        // Bind two listeners, both respond. Put one in the cache; the
        // other is just listed in members. Expect the cache's node to
        // be used (observable via which address logs the accept).
        let l1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let l2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a1 = l1.local_addr().unwrap();
        let a2 = l2.local_addr().unwrap();

        let (tx, mut rx) = mpsc::channel::<&'static str>(2);
        let tx1 = tx.clone();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = l1.accept().await {
                let _ = tx1.send("l1").await;
                if let Ok(req) = read_request(&mut s).await {
                    let _ = write_response(
                        &mut s,
                        req.shard_id,
                        &ShardRpcResponse {
                            request_id: req.request_id,
                            payload: ShardResponse::Ok { rows: vec![] },
                        },
                    )
                    .await;
                }
            }
        });
        tokio::spawn(async move {
            if let Ok((mut s, _)) = l2.accept().await {
                let _ = tx.send("l2").await;
                if let Ok(req) = read_request(&mut s).await {
                    let _ = write_response(
                        &mut s,
                        req.shard_id,
                        &ShardRpcResponse {
                            request_id: req.request_id,
                            payload: ShardResponse::Ok { rows: vec![] },
                        },
                    )
                    .await;
                }
            }
        });

        let mut addrs = BTreeMap::new();
        addrs.insert(nid("n1"), a1);
        addrs.insert(nid("n2"), a2);
        let mut members = BTreeMap::new();
        members.insert(ShardId::new(0), vec![nid("n1"), nid("n2")]);
        let cache = Arc::new(LeaderCache::new());
        cache.update(ShardId::new(0), nid("n2"));

        let client = TcpShardClient::new(
            TcpShardClientConfig::default(),
            Handle::current(),
            addrs,
            members,
            cache,
        );
        let _ = tokio::task::spawn_blocking(move || {
            client.execute(
                ShardId::new(0),
                "",
                &serde_json::Map::new(),
                1,
                Instant::now() + Duration::from_secs(3),
            )
        })
        .await
        .unwrap();

        let first = rx.recv().await.unwrap();
        assert_eq!(first, "l2", "cache should have routed to n2 first");
    }
}
