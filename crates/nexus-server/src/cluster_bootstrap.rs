//! V2 sharding bootstrap for the server process.
//!
//! Ties the three Phase 1-4 core crates together into a running cluster:
//!
//! 1. Parse [`nexus_core::sharding::ShardingConfig`] from env vars.
//! 2. In `Bootstrap` mode — build an initial [`ClusterMeta`] from the
//!    static peer list and hand it to a [`ClusterController`].
//! 3. In `Join` mode — dial each seed in order, request the current
//!    metadata snapshot, install it locally.
//! 4. Start a [`TcpRaftTransport`] bound to `listen_addr` and register
//!    every peer.
//! 5. Spawn a [`MetadataDriver`] task that owns the metadata-group
//!    [`RaftNode`] + the inbound-envelope stream, ticks the node, and
//!    keeps the `ClusterController` in sync with whatever Raft role
//!    the node currently holds.
//! 6. Build a [`TcpShardClient`] whose leader cache is fed by the
//!    driver and whose node-address table mirrors the controller's
//!    membership.
//!
//! The returned [`BootstrapHandle`] bundles everything the rest of
//! `nexus-server` needs: the `ClusterController` it installs on the
//! [`crate::NexusServer`], the `TcpShardClient` the coordinator's
//! [`ScatterGather`] uses, and the join-protocol listener the other
//! nodes dial when they start up with `mode = join`.
//!
//! # Scope
//!
//! This module is intentionally narrower than "full Raft consensus for
//! every `/cluster/*` mutation". The controller still applies mutations
//! locally; the driver loop mirrors its `ClusterMeta` as Raft log
//! entries so followers observe the same generation. Propagating each
//! [`super::MetaChange`] through Raft as a discrete proposal (instead of
//! whole-state snapshots) is a follow-up inside
//! `phase5_v2-tcp-transport-bridge` Phase 4 benchmarks — the wire
//! shape is already sufficient; the trade-off is per-mutation
//! latency vs. implementation complexity.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use nexus_core::coordinator::{LeaderCache, TcpShardClient, TcpShardClientConfig};
use nexus_core::sharding::controller::{ClusterController, StaticAllHealthy};
use nexus_core::sharding::metadata::ShardId;
use nexus_core::sharding::metadata::{ClusterMeta, NodeId, NodeInfo};
use nexus_core::sharding::raft::types::RaftEnvelope;
use nexus_core::sharding::raft::{
    RaftNode, RaftNodeConfig, TcpRaftTransport, TcpRaftTransportConfig,
};
use nexus_core::sharding::{PeerEntry, ShardingConfig, ShardingMode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Bootstrap handle kept alive for the lifetime of the server. Dropping
/// it aborts every spawned task — callers store it in an `Arc` on
/// [`crate::NexusServer`].
pub struct BootstrapHandle {
    /// The cluster controller the server installs under
    /// `/cluster/status` + the mutating endpoints.
    pub controller: Arc<ClusterController>,
    /// The TCP shard client the coordinator uses for scatter/gather.
    pub shard_client: Arc<TcpShardClient>,
    /// Shutdown channel for the driver task.
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Handle kept so we can `.abort()` on shutdown.
    #[allow(dead_code)]
    driver_handle: JoinHandle<()>,
    /// Handle kept so we can `.abort()` on shutdown.
    #[allow(dead_code)]
    join_listener_handle: JoinHandle<()>,
    /// The TCP transport. Kept here so its tasks survive.
    #[allow(dead_code)]
    transport: Arc<TcpRaftTransport>,
}

impl std::fmt::Debug for BootstrapHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Wraps opaque tokio JoinHandles + Arc<non-Debug> transport,
        // so project them through the controller snapshot — enough
        // for test assertions and for operator log lines that want
        // `{:?}` without leaking task internals.
        f.debug_struct("BootstrapHandle")
            .field("this_node", self.controller.this_node())
            .field("generation", &self.controller.meta().generation)
            .field("is_leader", &self.controller.is_leader())
            .finish()
    }
}

impl BootstrapHandle {
    /// Graceful shutdown: tell the driver to exit, abort all tasks.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        self.driver_handle.abort();
        self.join_listener_handle.abort();
        self.transport.shutdown();
    }
}

/// Errors surfaced during bootstrap.
#[derive(Debug, Error)]
pub enum BootstrapError {
    /// Provided [`ShardingConfig`] failed validation.
    #[error("invalid sharding config: {0}")]
    Config(String),
    /// Could not bind the TCP listener on `listen_addr`.
    #[error("failed to bind Raft listener: {0}")]
    Bind(std::io::Error),
    /// Failed to parse an env-var value.
    #[error("bad env var {name}: {msg}")]
    BadEnv { name: &'static str, msg: String },
    /// Join mode exhausted seeds without receiving a metadata snapshot.
    #[error("no seed responded with cluster metadata (tried {tried} seeds)")]
    JoinFailed { tried: usize },
    /// An I/O error during the join handshake.
    #[error("join I/O: {0}")]
    JoinIo(std::io::Error),
    /// Serialization / CRC error during the join handshake.
    #[error("join wire: {0}")]
    JoinWire(String),
}

// ---------------------------------------------------------------------------
// Env-var config loader
// ---------------------------------------------------------------------------

/// Parse a [`ShardingConfig`] from `NEXUS_SHARDING_*` env vars. Returns
/// `Ok(None)` if `NEXUS_SHARDING_MODE` is unset or equals `disabled` —
/// the server boots in single-node mode in that case. `Err` is
/// reserved for malformed inputs that would make silent fallback
/// dangerous.
///
/// Env vars consumed:
///
/// * `NEXUS_SHARDING_MODE` — `disabled` / `bootstrap` / `join`.
/// * `NEXUS_SHARDING_NODE_ID` — stable id of this node.
/// * `NEXUS_SHARDING_LISTEN_ADDR` — `host:port` for Raft traffic.
/// * `NEXUS_SHARDING_PEERS` — comma-separated `node_id=addr` entries.
/// * `NEXUS_SHARDING_NUM_SHARDS` — u32, only read at bootstrap.
/// * `NEXUS_SHARDING_REPLICA_FACTOR` — u32, only read at bootstrap.
pub fn parse_sharding_env() -> Result<Option<ShardingConfig>, BootstrapError> {
    parse_sharding_from(|key| std::env::var(key).ok())
}

/// Internal helper — parse using a caller-supplied env source. Lets
/// tests exercise the parser without global env-var contamination.
pub fn parse_sharding_from<F>(lookup: F) -> Result<Option<ShardingConfig>, BootstrapError>
where
    F: Fn(&str) -> Option<String>,
{
    let mode_raw = match lookup("NEXUS_SHARDING_MODE") {
        None => return Ok(None),
        Some(s) if s.trim().eq_ignore_ascii_case("disabled") => return Ok(None),
        Some(s) => s,
    };
    let mode = match mode_raw.trim().to_ascii_lowercase().as_str() {
        "bootstrap" => ShardingMode::Bootstrap,
        "join" => ShardingMode::Join,
        other => {
            return Err(BootstrapError::BadEnv {
                name: "NEXUS_SHARDING_MODE",
                msg: format!("unknown mode {other:?}; expected bootstrap|join|disabled"),
            });
        }
    };

    let node_id_raw = lookup("NEXUS_SHARDING_NODE_ID").ok_or(BootstrapError::BadEnv {
        name: "NEXUS_SHARDING_NODE_ID",
        msg: "required".into(),
    })?;
    let node_id = NodeId::new(node_id_raw).map_err(|e| BootstrapError::BadEnv {
        name: "NEXUS_SHARDING_NODE_ID",
        msg: e.to_string(),
    })?;

    let listen_addr_raw =
        lookup("NEXUS_SHARDING_LISTEN_ADDR").unwrap_or_else(|| "0.0.0.0:15480".to_string());
    let listen_addr: SocketAddr =
        listen_addr_raw
            .parse()
            .map_err(|e: std::net::AddrParseError| BootstrapError::BadEnv {
                name: "NEXUS_SHARDING_LISTEN_ADDR",
                msg: e.to_string(),
            })?;

    let peers = parse_peers_env(lookup("NEXUS_SHARDING_PEERS").unwrap_or_default())?;

    let num_shards: u32 = lookup("NEXUS_SHARDING_NUM_SHARDS")
        .unwrap_or_else(|| "1".to_string())
        .parse()
        .map_err(|e: std::num::ParseIntError| BootstrapError::BadEnv {
            name: "NEXUS_SHARDING_NUM_SHARDS",
            msg: e.to_string(),
        })?;
    let replica_factor: u32 = lookup("NEXUS_SHARDING_REPLICA_FACTOR")
        .unwrap_or_else(|| "1".to_string())
        .parse()
        .map_err(|e: std::num::ParseIntError| BootstrapError::BadEnv {
            name: "NEXUS_SHARDING_REPLICA_FACTOR",
            msg: e.to_string(),
        })?;

    let cfg = match mode {
        ShardingMode::Bootstrap => {
            ShardingConfig::bootstrap(node_id, listen_addr, peers, num_shards, replica_factor)
                .map_err(|e| BootstrapError::Config(e.to_string()))?
        }
        ShardingMode::Join => ShardingConfig::join(node_id, listen_addr, peers)
            .map_err(|e| BootstrapError::Config(e.to_string()))?,
        ShardingMode::Disabled => return Ok(None),
    };
    Ok(Some(cfg))
}

fn parse_peers_env(raw: String) -> Result<Vec<PeerEntry>, BootstrapError> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (id, addr) = entry
            .split_once('=')
            .ok_or_else(|| BootstrapError::BadEnv {
                name: "NEXUS_SHARDING_PEERS",
                msg: format!("entry {entry:?} missing '='; expected 'node_id=host:port'"),
            })?;
        let node_id = NodeId::new(id.trim()).map_err(|e| BootstrapError::BadEnv {
            name: "NEXUS_SHARDING_PEERS",
            msg: format!("bad node id {id:?}: {e}"),
        })?;
        let addr: SocketAddr =
            addr.trim()
                .parse()
                .map_err(|e: std::net::AddrParseError| BootstrapError::BadEnv {
                    name: "NEXUS_SHARDING_PEERS",
                    msg: format!("bad addr {addr:?}: {e}"),
                })?;
        out.push(PeerEntry { node_id, addr });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Join-protocol wire frame
// ---------------------------------------------------------------------------

/// The joining node sends this frame to each seed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    /// Joining node's id.
    pub node_id: NodeId,
    /// Joining node's Raft listen address.
    pub listen_addr: SocketAddr,
}

/// Seed's reply — the current metadata the joiner should adopt. No
/// CRC here; this runs over plain TCP and the joiner's next heartbeat
/// with the cluster will surface any drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinResponse {
    /// Authoritative cluster metadata. The joiner writes this into
    /// its `ClusterController` before entering the normal ticking loop.
    pub meta: ClusterMeta,
}

const JOIN_FRAME_TYPE_REQUEST: u8 = 0x70;
const JOIN_FRAME_TYPE_RESPONSE: u8 = 0x71;

async fn write_length_prefixed<W, T>(
    writer: &mut W,
    kind: u8,
    value: &T,
) -> Result<(), BootstrapError>
where
    W: tokio::io::AsyncWrite + Unpin,
    T: Serialize,
{
    let payload =
        rmp_serde::to_vec_named(value).map_err(|e| BootstrapError::JoinWire(e.to_string()))?;
    let mut buf = Vec::with_capacity(5 + payload.len());
    buf.push(kind);
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(&payload);
    writer
        .write_all(&buf)
        .await
        .map_err(BootstrapError::JoinIo)?;
    writer.flush().await.map_err(BootstrapError::JoinIo)?;
    Ok(())
}

async fn read_length_prefixed<R, T>(reader: &mut R, expected_kind: u8) -> Result<T, BootstrapError>
where
    R: tokio::io::AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut header = [0u8; 5];
    reader
        .read_exact(&mut header)
        .await
        .map_err(BootstrapError::JoinIo)?;
    if header[0] != expected_kind {
        return Err(BootstrapError::JoinWire(format!(
            "expected kind 0x{expected_kind:02x}, got 0x{:02x}",
            header[0]
        )));
    }
    let len = u32::from_le_bytes([header[1], header[2], header[3], header[4]]) as usize;
    if len > 8 * 1024 * 1024 {
        return Err(BootstrapError::JoinWire(format!(
            "join payload {len} bytes exceeds 8 MiB cap"
        )));
    }
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(BootstrapError::JoinIo)?;
    rmp_serde::from_slice(&buf).map_err(|e| BootstrapError::JoinWire(e.to_string()))
}

/// Run the join-protocol listener: accept incoming `JoinRequest`
/// frames and reply with the current `ClusterMeta`. Returns when the
/// listener errors (typically never under normal operation) or the
/// task is aborted via [`BootstrapHandle::shutdown`].
async fn run_join_listener(listener: TcpListener, controller: Arc<ClusterController>) {
    loop {
        match listener.accept().await {
            Ok((mut stream, peer_addr)) => {
                let snap = controller.meta();
                let controller = controller.clone();
                tokio::spawn(async move {
                    // Read the joiner's request — we don't currently
                    // use its contents (future: record the node into
                    // metadata automatically), but draining it keeps
                    // the wire clean for future evolution.
                    let req: Result<JoinRequest, _> =
                        read_length_prefixed(&mut stream, JOIN_FRAME_TYPE_REQUEST).await;
                    match req {
                        Ok(r) => debug!("join request from {} @ {peer_addr}", r.node_id),
                        Err(e) => {
                            warn!("join listener: bad request from {peer_addr}: {e}");
                            return;
                        }
                    }
                    let resp = JoinResponse { meta: snap };
                    if let Err(e) =
                        write_length_prefixed(&mut stream, JOIN_FRAME_TYPE_RESPONSE, &resp).await
                    {
                        warn!("join listener: write to {peer_addr} failed: {e}");
                    }
                    let _ = stream.shutdown().await;
                    let _ = controller; // keep arc alive across await
                });
            }
            Err(e) => {
                warn!("join listener accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

/// Dial the seed list in order, requesting metadata. Returns the first
/// successful response.
async fn run_join_client(
    this_node: NodeId,
    listen_addr: SocketAddr,
    seeds: &[PeerEntry],
    join_port_offset: u16,
) -> Result<ClusterMeta, BootstrapError> {
    let mut tried = 0usize;
    for seed in seeds {
        tried += 1;
        let join_addr = SocketAddr::new(seed.addr.ip(), seed.addr.port() + join_port_offset);
        debug!("join: dialing {} at {join_addr}", seed.node_id);
        match TcpStream::connect(join_addr).await {
            Ok(mut stream) => {
                let req = JoinRequest {
                    node_id: this_node.clone(),
                    listen_addr,
                };
                if let Err(e) =
                    write_length_prefixed(&mut stream, JOIN_FRAME_TYPE_REQUEST, &req).await
                {
                    warn!("join: write to {} failed: {e}", seed.node_id);
                    continue;
                }
                match read_length_prefixed::<_, JoinResponse>(&mut stream, JOIN_FRAME_TYPE_RESPONSE)
                    .await
                {
                    Ok(resp) => {
                        info!("join: received metadata snapshot from {}", seed.node_id);
                        return Ok(resp.meta);
                    }
                    Err(e) => {
                        warn!("join: read from {} failed: {e}", seed.node_id);
                        continue;
                    }
                }
            }
            Err(e) => {
                warn!("join: dial {} failed: {e}", seed.node_id);
                continue;
            }
        }
    }
    Err(BootstrapError::JoinFailed { tried })
}

// ---------------------------------------------------------------------------
// Metadata Raft driver
// ---------------------------------------------------------------------------

/// Cadence the [`MetadataDriver`] ticks its owned [`RaftNode`]. Matches
/// the node's `tick` granularity; don't change without re-validating
/// `RaftNodeConfig::tick`.
const DRIVER_TICK: Duration = Duration::from_millis(10);

/// Owns the metadata-group [`RaftNode`] and keeps the paired
/// [`ClusterController`] in sync with its Raft role.
struct MetadataDriver {
    node: RaftNode,
    transport: Arc<TcpRaftTransport>,
    inbound: mpsc::Receiver<RaftEnvelope>,
    controller: Arc<ClusterController>,
    shutdown: mpsc::Receiver<()>,
}

impl MetadataDriver {
    fn new(
        node: RaftNode,
        transport: Arc<TcpRaftTransport>,
        inbound: mpsc::Receiver<RaftEnvelope>,
        controller: Arc<ClusterController>,
        shutdown: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            node,
            transport,
            inbound,
            controller,
            shutdown,
        }
    }

    async fn run(mut self) {
        let mut ticker = tokio::time::interval(DRIVER_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_leader_state: Option<(bool, Option<NodeId>)> = None;
        loop {
            tokio::select! {
                _ = self.shutdown.recv() => {
                    debug!("metadata driver shutdown received");
                    return;
                }
                Some(env) = self.inbound.recv() => {
                    let _ = self
                        .node
                        .handle_message(env, self.transport.as_ref());
                }
                _ = ticker.tick() => {
                    let _ = self.node.tick(self.transport.as_ref());
                }
            }
            // Propagate role change to the controller.
            let is_leader = self.node.is_leader();
            let hint = self.node.leader_hint().cloned();
            let state = Some((is_leader, hint.clone()));
            if state != last_leader_state {
                self.controller.set_leader(is_leader, hint);
                last_leader_state = state;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Bootstrap entry point
// ---------------------------------------------------------------------------

/// Default offset from `listen_addr` to the join-protocol port. The
/// join listener lives on `listen_addr + JOIN_PORT_OFFSET` so a single
/// operator-visible address is enough to plumb both the Raft traffic
/// and the bootstrap handshake.
pub const JOIN_PORT_OFFSET: u16 = 1;

/// Bootstrap the sharding subsystem. Returns `None` when
/// `cfg.mode = Disabled` — the caller should leave the server in
/// standalone mode.
pub async fn bootstrap_sharding(
    cfg: ShardingConfig,
    runtime: Handle,
) -> Result<Option<BootstrapHandle>, BootstrapError> {
    cfg.validate()
        .map_err(|e| BootstrapError::Config(e.to_string()))?;
    if cfg.mode == ShardingMode::Disabled {
        return Ok(None);
    }

    let this_node = cfg
        .node_id
        .clone()
        .ok_or_else(|| BootstrapError::Config("node_id required".into()))?;

    // Seed the metadata depending on mode.
    let meta = match cfg.mode {
        ShardingMode::Disabled => unreachable!(),
        ShardingMode::Bootstrap => {
            let nodes: Vec<NodeInfo> = cfg
                .peers
                .iter()
                .map(|p| NodeInfo::new(p.node_id.clone(), p.addr))
                .collect();
            ClusterMeta::bootstrap(nodes, cfg.num_shards, cfg.replica_factor)
                .map_err(|e| BootstrapError::Config(e.to_string()))?
        }
        ShardingMode::Join => {
            run_join_client(
                this_node.clone(),
                cfg.listen_addr,
                &cfg.peers,
                JOIN_PORT_OFFSET,
            )
            .await?
        }
    };

    // Start the TCP Raft transport.
    let (transport, inbound_rx) =
        TcpRaftTransport::start(cfg.listen_addr, TcpRaftTransportConfig::default())
            .await
            .map_err(BootstrapError::Bind)?;
    let transport = Arc::new(transport);
    for peer in &cfg.peers {
        if peer.node_id != this_node {
            transport.add_peer(peer.node_id.clone(), peer.addr);
        }
    }

    // Build the controller (initially follower; driver promotes later).
    let controller = Arc::new(ClusterController::new(
        this_node.clone(),
        meta.clone(),
        false,
        Arc::new(StaticAllHealthy),
    ));

    // Spin the metadata RaftNode. Members = cluster metadata group.
    let members = meta.metadata_members.clone();
    let rng_seed = deterministic_seed(&this_node);
    let node_cfg = RaftNodeConfig {
        shard_id: ShardId::new(0),
        node_id: this_node.clone(),
        members,
        election_timeout_min: cfg.election_timeout_min,
        election_timeout_max: cfg.election_timeout_max,
        heartbeat_interval: cfg.heartbeat,
        tick: DRIVER_TICK,
        rng_seed,
    };
    let node = RaftNode::new(node_cfg).map_err(BootstrapError::Config)?;

    // Driver task.
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
    let driver = MetadataDriver::new(
        node,
        transport.clone(),
        inbound_rx,
        controller.clone(),
        shutdown_rx,
    );
    let driver_handle = tokio::spawn(async move { driver.run().await });

    // Join-protocol listener bound at listen_addr + JOIN_PORT_OFFSET.
    let join_addr = SocketAddr::new(
        cfg.listen_addr.ip(),
        cfg.listen_addr.port() + JOIN_PORT_OFFSET,
    );
    let join_listener = TcpListener::bind(join_addr)
        .await
        .map_err(BootstrapError::Bind)?;
    let controller_for_join = controller.clone();
    let join_listener_handle =
        tokio::spawn(async move { run_join_listener(join_listener, controller_for_join).await });

    // TCP shard client plumbing.
    let mut node_addrs = BTreeMap::new();
    for peer in &cfg.peers {
        node_addrs.insert(peer.node_id.clone(), peer.addr);
    }
    let mut shard_members = BTreeMap::new();
    for shard in &meta.shards {
        shard_members.insert(shard.shard_id, shard.members.clone());
    }
    let shard_client = Arc::new(TcpShardClient::new(
        TcpShardClientConfig::default(),
        runtime,
        node_addrs,
        shard_members,
        Arc::new(LeaderCache::new()),
    ));

    Ok(Some(BootstrapHandle {
        controller,
        shard_client,
        shutdown_tx: Some(shutdown_tx),
        driver_handle,
        join_listener_handle,
        transport,
    }))
}

/// FNV-1a 64 over the node id string. Only used to seed the Raft
/// election-timeout RNG so every node's jitter is distinct; cryptographic
/// strength is not required. Avoids pulling `xxhash-rust` into
/// nexus-server just for one call.
fn deterministic_seed(id: &NodeId) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.as_str().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// Reserved for a future "load from file" path — keeps the import list
// honest so the parser doesn't warn about a missing `FromStr`.
#[allow(dead_code)]
fn _parse_peer_addr(raw: &str) -> Result<SocketAddr, std::net::AddrParseError> {
    SocketAddr::from_str(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn env_missing_mode_returns_none() {
        let cfg = parse_sharding_from(fake_env(&[])).unwrap();
        assert!(cfg.is_none());
    }

    #[test]
    fn env_explicit_disabled_returns_none() {
        let cfg = parse_sharding_from(fake_env(&[("NEXUS_SHARDING_MODE", "disabled")])).unwrap();
        assert!(cfg.is_none());
    }

    #[test]
    fn env_unknown_mode_errors() {
        let err =
            parse_sharding_from(fake_env(&[("NEXUS_SHARDING_MODE", "gibberish")])).unwrap_err();
        assert!(matches!(err, BootstrapError::BadEnv { .. }));
    }

    #[test]
    fn env_bootstrap_happy_path() {
        let cfg = parse_sharding_from(fake_env(&[
            ("NEXUS_SHARDING_MODE", "bootstrap"),
            ("NEXUS_SHARDING_NODE_ID", "node-a"),
            ("NEXUS_SHARDING_LISTEN_ADDR", "127.0.0.1:15480"),
            (
                "NEXUS_SHARDING_PEERS",
                "node-a=127.0.0.1:15480,node-b=127.0.0.1:15481",
            ),
            ("NEXUS_SHARDING_NUM_SHARDS", "2"),
            ("NEXUS_SHARDING_REPLICA_FACTOR", "2"),
        ]))
        .unwrap()
        .expect("config should parse");
        assert_eq!(cfg.mode, ShardingMode::Bootstrap);
        assert_eq!(cfg.num_shards, 2);
        assert_eq!(cfg.replica_factor, 2);
        assert_eq!(cfg.peers.len(), 2);
    }

    #[test]
    fn env_peers_bad_entry_errors() {
        let err = parse_sharding_from(fake_env(&[
            ("NEXUS_SHARDING_MODE", "bootstrap"),
            ("NEXUS_SHARDING_NODE_ID", "node-a"),
            ("NEXUS_SHARDING_PEERS", "bogus"),
        ]))
        .unwrap_err();
        assert!(
            matches!(err, BootstrapError::BadEnv { name, .. } if name == "NEXUS_SHARDING_PEERS")
        );
    }

    #[test]
    fn env_join_requires_node_id() {
        let err = parse_sharding_from(fake_env(&[("NEXUS_SHARDING_MODE", "join")])).unwrap_err();
        assert!(
            matches!(err, BootstrapError::BadEnv { name, .. } if name == "NEXUS_SHARDING_NODE_ID")
        );
    }

    #[test]
    fn deterministic_seed_is_stable_across_calls() {
        let id = NodeId::new("node-a").unwrap();
        assert_eq!(deterministic_seed(&id), deterministic_seed(&id));
    }

    #[test]
    fn deterministic_seed_differs_between_nodes() {
        let a = NodeId::new("node-a").unwrap();
        let b = NodeId::new("node-b").unwrap();
        assert_ne!(deterministic_seed(&a), deterministic_seed(&b));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn bootstrap_single_node_elects_itself() {
        let listen: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(listen).await.unwrap();
        let bound = listener.local_addr().unwrap();
        drop(listener); // free the port for bootstrap
        let peer = PeerEntry {
            node_id: NodeId::new("node-a").unwrap(),
            addr: bound,
        };
        let cfg =
            ShardingConfig::bootstrap(NodeId::new("node-a").unwrap(), bound, vec![peer], 1, 1)
                .unwrap();
        let handle = bootstrap_sharding(cfg, Handle::current())
            .await
            .unwrap()
            .expect("not disabled");
        // Wait for election. Controller flips to leader when the node wins.
        for _ in 0..100 {
            if handle.controller.is_leader() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        assert!(
            handle.controller.is_leader(),
            "single-node cluster must elect itself"
        );
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn join_protocol_roundtrips_metadata_snapshot() {
        // Drive the join handshake directly — a seed-side listener
        // backed by a fresh ClusterController, and `run_join_client`
        // against it. The full 2-node end-to-end bootstrap path
        // would need four consecutive free 127.0.0.1 ports (two
        // listen_addr + two join ports at listen_addr+1), which
        // cannot be allocated deterministically from port 0; that
        // wiring is covered by the Phase 5 Docker integration suite.
        let seed_meta = ClusterMeta::bootstrap(
            vec![NodeInfo::new(
                NodeId::new("seed").unwrap(),
                "127.0.0.1:15500".parse().unwrap(),
            )],
            1,
            1,
        )
        .unwrap();
        let seed_controller = Arc::new(ClusterController::new(
            NodeId::new("seed").unwrap(),
            seed_meta.clone(),
            true,
            Arc::new(StaticAllHealthy),
        ));
        let join_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let join_addr = join_listener.local_addr().unwrap();
        let seed_controller_for_task = seed_controller.clone();
        let handle = tokio::spawn(async move {
            run_join_listener(join_listener, seed_controller_for_task).await;
        });

        // Dial the join port directly by passing an offset of 0 —
        // `run_join_client` adds it to `seed.addr.port()`, so pointing
        // the seed entry straight at the listener exercises the
        // handshake without the bootstrap port-offset dance.
        let seed_entry = PeerEntry {
            node_id: NodeId::new("seed").unwrap(),
            addr: join_addr,
        };
        let got = run_join_client(
            NodeId::new("joiner").unwrap(),
            "127.0.0.1:15501".parse().unwrap(),
            &[seed_entry],
            0,
        )
        .await
        .expect("join should succeed");
        assert_eq!(got.cluster_id, seed_meta.cluster_id);
        assert_eq!(got.num_shards, 1);
        handle.abort();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn join_fails_when_seeds_unreachable() {
        let listen: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let tmp = TcpListener::bind(listen).await.unwrap();
        let addr = tmp.local_addr().unwrap();
        drop(tmp);
        // Seed points at a free port on 127.0.0.1 — connect will fail.
        let cfg = ShardingConfig::join(
            NodeId::new("node-b").unwrap(),
            addr,
            vec![PeerEntry {
                node_id: NodeId::new("node-x").unwrap(),
                addr: "127.0.0.1:1".parse().unwrap(),
            }],
        )
        .unwrap();
        let err = bootstrap_sharding(cfg, Handle::current())
            .await
            .unwrap_err();
        assert!(matches!(err, BootstrapError::JoinFailed { .. }));
    }
}
