//! Sharding configuration.
//!
//! Parsed from the server's TOML config under `[cluster.sharding]`. The
//! type is deliberately small: everything an operator can turn on / off,
//! nothing that has to match across nodes. Cross-node concerns
//! (membership, shard count) live in [`super::metadata::ClusterMeta`]
//! and are committed through the metadata Raft group.

use std::net::SocketAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::metadata::{ClusterMetaError, NodeId};

/// What mode this node is running in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShardingMode {
    /// Sharding disabled — classic single-node Nexus.
    #[default]
    Disabled,
    /// Bootstrap this node as part of a fresh cluster.
    Bootstrap,
    /// Join an existing cluster via the listed peers.
    Join,
}

/// A peer entry parsed from the `peers = [...]` list. Format is
/// `node_id=host:port`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerEntry {
    /// Peer's stable node id.
    pub node_id: NodeId,
    /// Peer's Raft/coordinator socket address.
    pub addr: SocketAddr,
}

/// Top-level sharding config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardingConfig {
    /// Mode for this node.
    #[serde(default)]
    pub mode: ShardingMode,
    /// Stable node id for this process. Required unless
    /// `mode = Disabled`.
    #[serde(default)]
    pub node_id: Option<NodeId>,
    /// Socket this node listens on for Raft + coordinator traffic.
    /// Default: `0.0.0.0:15480`.
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,
    /// Peers known at bootstrap time. For `Bootstrap`, this is the
    /// initial member set. For `Join`, these are seed peers the join
    /// request fans out to.
    #[serde(default)]
    pub peers: Vec<PeerEntry>,
    /// Number of data shards. Only read at `Bootstrap`; ignored on
    /// `Join` (the joining node reads the authoritative count from
    /// cluster metadata).
    #[serde(default = "default_num_shards")]
    pub num_shards: u32,
    /// Raft group size per shard. Same rule as `num_shards` — only
    /// read on Bootstrap.
    #[serde(default = "default_replica_factor")]
    pub replica_factor: u32,
    /// Lower bound of the election-timeout randomization window.
    #[serde(default = "default_election_min")]
    pub election_timeout_min: Duration,
    /// Upper bound of the election-timeout randomization window.
    #[serde(default = "default_election_max")]
    pub election_timeout_max: Duration,
    /// Heartbeat interval (leader → followers).
    #[serde(default = "default_heartbeat")]
    pub heartbeat: Duration,
    /// Trigger a snapshot + log compaction after this many appended
    /// log entries.
    #[serde(default = "default_snapshot_threshold")]
    pub snapshot_log_size_threshold: u64,
    /// Scatter/gather timeout applied at the coordinator.
    #[serde(default = "default_query_timeout")]
    pub query_timeout: Duration,
    /// Upper bound on remote-node fetches per query. Protects against
    /// runaway variable-length traversals.
    #[serde(default = "default_max_cross_shard_rpcs")]
    pub max_cross_shard_rpcs_per_query: u32,
    /// Max cross-shard cache entries.
    #[serde(default = "default_cross_shard_cache_size")]
    pub cross_shard_cache_size: usize,
    /// Cross-shard cache TTL safety net.
    #[serde(default = "default_cross_shard_cache_ttl")]
    pub cross_shard_cache_ttl: Duration,
}

fn default_listen_addr() -> SocketAddr {
    "0.0.0.0:15480".parse().expect("valid default listen addr")
}
fn default_num_shards() -> u32 {
    1
}
fn default_replica_factor() -> u32 {
    1
}
fn default_election_min() -> Duration {
    Duration::from_millis(500)
}
fn default_election_max() -> Duration {
    Duration::from_millis(1_000)
}
fn default_heartbeat() -> Duration {
    Duration::from_millis(100)
}
fn default_snapshot_threshold() -> u64 {
    10_000
}
fn default_query_timeout() -> Duration {
    Duration::from_secs(30)
}
fn default_max_cross_shard_rpcs() -> u32 {
    1_000
}
fn default_cross_shard_cache_size() -> usize {
    10_000
}
fn default_cross_shard_cache_ttl() -> Duration {
    Duration::from_secs(30)
}

impl Default for ShardingConfig {
    fn default() -> Self {
        Self {
            mode: ShardingMode::Disabled,
            node_id: None,
            listen_addr: default_listen_addr(),
            peers: Vec::new(),
            num_shards: default_num_shards(),
            replica_factor: default_replica_factor(),
            election_timeout_min: default_election_min(),
            election_timeout_max: default_election_max(),
            heartbeat: default_heartbeat(),
            snapshot_log_size_threshold: default_snapshot_threshold(),
            query_timeout: default_query_timeout(),
            max_cross_shard_rpcs_per_query: default_max_cross_shard_rpcs(),
            cross_shard_cache_size: default_cross_shard_cache_size(),
            cross_shard_cache_ttl: default_cross_shard_cache_ttl(),
        }
    }
}

impl ShardingConfig {
    /// Disabled config (sharding off). Mirrors
    /// [`crate::replication::ReplicationConfig::standalone`].
    #[must_use]
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Build a bootstrap config for a node. `peers` is the initial member
    /// set including this node itself.
    pub fn bootstrap(
        node_id: NodeId,
        listen_addr: SocketAddr,
        peers: Vec<PeerEntry>,
        num_shards: u32,
        replica_factor: u32,
    ) -> Result<Self, ClusterMetaError> {
        let cfg = Self {
            mode: ShardingMode::Bootstrap,
            node_id: Some(node_id),
            listen_addr,
            peers,
            num_shards,
            replica_factor,
            ..Self::default()
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Build a join config. `peers` is the list of seed nodes to contact.
    pub fn join(
        node_id: NodeId,
        listen_addr: SocketAddr,
        peers: Vec<PeerEntry>,
    ) -> Result<Self, ClusterMetaError> {
        let cfg = Self {
            mode: ShardingMode::Join,
            node_id: Some(node_id),
            listen_addr,
            peers,
            ..Self::default()
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Enforce the cross-mode invariants the type alone can't encode.
    pub fn validate(&self) -> Result<(), ClusterMetaError> {
        match self.mode {
            ShardingMode::Disabled => {
                // No further requirements.
            }
            ShardingMode::Bootstrap => {
                if self.node_id.is_none() {
                    return Err(ClusterMetaError::Bootstrap(
                        "node_id is required in Bootstrap mode".into(),
                    ));
                }
                if self.peers.is_empty() {
                    return Err(ClusterMetaError::Bootstrap(
                        "at least one peer entry (including self) is required in Bootstrap mode"
                            .into(),
                    ));
                }
                if self.num_shards == 0 {
                    return Err(ClusterMetaError::Bootstrap(
                        "num_shards must be >= 1".into(),
                    ));
                }
                if self.replica_factor == 0 {
                    return Err(ClusterMetaError::Bootstrap(
                        "replica_factor must be >= 1".into(),
                    ));
                }
                if self.replica_factor as usize > self.peers.len() {
                    return Err(ClusterMetaError::Bootstrap(format!(
                        "replica_factor={} exceeds peers.len()={}",
                        self.replica_factor,
                        self.peers.len()
                    )));
                }
                if let Some(ref id) = self.node_id {
                    if !self.peers.iter().any(|p| &p.node_id == id) {
                        return Err(ClusterMetaError::Bootstrap(
                            "own node_id must appear in peers for Bootstrap".into(),
                        ));
                    }
                }
            }
            ShardingMode::Join => {
                if self.node_id.is_none() {
                    return Err(ClusterMetaError::Bootstrap(
                        "node_id is required in Join mode".into(),
                    ));
                }
                if self.peers.is_empty() {
                    return Err(ClusterMetaError::Bootstrap(
                        "at least one seed peer is required in Join mode".into(),
                    ));
                }
            }
        }
        if self.election_timeout_min >= self.election_timeout_max {
            return Err(ClusterMetaError::Bootstrap(
                "election_timeout_min must be strictly less than election_timeout_max".into(),
            ));
        }
        if self.heartbeat >= self.election_timeout_min {
            return Err(ClusterMetaError::Bootstrap(
                "heartbeat must be strictly less than election_timeout_min".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(s: &str) -> NodeId {
        NodeId::new(s).unwrap()
    }

    fn peer(id: &str, port: u16) -> PeerEntry {
        PeerEntry {
            node_id: nid(id),
            addr: format!("127.0.0.1:{port}").parse().unwrap(),
        }
    }

    #[test]
    fn default_is_disabled() {
        let cfg = ShardingConfig::default();
        assert_eq!(cfg.mode, ShardingMode::Disabled);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn bootstrap_requires_node_id_in_peers() {
        let err = ShardingConfig::bootstrap(
            nid("node-a"),
            "0.0.0.0:15480".parse().unwrap(),
            vec![peer("node-b", 15481)],
            1,
            1,
        )
        .unwrap_err();
        assert!(matches!(err, ClusterMetaError::Bootstrap(_)));
    }

    #[test]
    fn bootstrap_rejects_replica_factor_over_peers() {
        let err = ShardingConfig::bootstrap(
            nid("node-a"),
            "0.0.0.0:15480".parse().unwrap(),
            vec![peer("node-a", 15480)],
            1,
            2,
        )
        .unwrap_err();
        assert!(matches!(err, ClusterMetaError::Bootstrap(_)));
    }

    #[test]
    fn bootstrap_happy_path() {
        let cfg = ShardingConfig::bootstrap(
            nid("node-a"),
            "0.0.0.0:15480".parse().unwrap(),
            vec![
                peer("node-a", 15480),
                peer("node-b", 15481),
                peer("node-c", 15482),
            ],
            3,
            3,
        )
        .unwrap();
        assert_eq!(cfg.mode, ShardingMode::Bootstrap);
        assert_eq!(cfg.num_shards, 3);
        assert_eq!(cfg.replica_factor, 3);
    }

    #[test]
    fn join_requires_peers() {
        let err = ShardingConfig::join(nid("node-a"), "0.0.0.0:15480".parse().unwrap(), vec![])
            .unwrap_err();
        assert!(matches!(err, ClusterMetaError::Bootstrap(_)));
    }

    #[test]
    fn heartbeat_must_be_less_than_election_min() {
        let mut cfg = ShardingConfig::bootstrap(
            nid("node-a"),
            "0.0.0.0:15480".parse().unwrap(),
            vec![peer("node-a", 15480)],
            1,
            1,
        )
        .unwrap();
        cfg.heartbeat = cfg.election_timeout_min;
        assert!(matches!(
            cfg.validate(),
            Err(ClusterMetaError::Bootstrap(_))
        ));
    }

    #[test]
    fn election_min_must_be_less_than_max() {
        let mut cfg = ShardingConfig::bootstrap(
            nid("node-a"),
            "0.0.0.0:15480".parse().unwrap(),
            vec![peer("node-a", 15480)],
            1,
            1,
        )
        .unwrap();
        cfg.election_timeout_min = cfg.election_timeout_max;
        assert!(matches!(
            cfg.validate(),
            Err(ClusterMetaError::Bootstrap(_))
        ));
    }

    #[test]
    fn config_roundtrips_through_json() {
        let cfg = ShardingConfig::bootstrap(
            nid("node-a"),
            "0.0.0.0:15480".parse().unwrap(),
            vec![peer("node-a", 15480), peer("node-b", 15481)],
            2,
            2,
        )
        .unwrap();
        let s = serde_json::to_string(&cfg).unwrap();
        let back: ShardingConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(back.mode, ShardingMode::Bootstrap);
        assert_eq!(back.peers.len(), 2);
    }
}
