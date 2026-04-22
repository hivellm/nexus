//! Cluster metadata data model.
//!
//! A [`ClusterMeta`] is the authoritative description of a V2 cluster:
//! who the nodes are, how many shards exist, which nodes hold each shard,
//! and the monotonic `generation` number that every metadata mutation
//! advances. This struct is what the metadata Raft group's apply loop
//! maintains; snapshots of it are shipped to new metadata replicas.
//!
//! The type is kept Raft-agnostic on purpose: Phase 1 exercises it as a
//! pure value type with unit tests; Phase 2 wraps it in a Raft state
//! machine without having to change this module.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ShardId / NodeId newtypes
// ---------------------------------------------------------------------------

/// A shard identifier — a dense 0-indexed u32.
///
/// Newtype rather than a raw `u32` so call sites can't accidentally hash
/// the wrong integer into a shard slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ShardId(u32);

impl ShardId {
    /// Construct a shard id from a raw `u32`.
    #[inline]
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Raw u32 representation.
    #[inline]
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for ShardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shard-{}", self.0)
    }
}

/// Logical identifier for a node in the cluster. Stable across restarts;
/// set via operator config (e.g. `node-a`, `node-b`). Different from the
/// transient TCP address, which is recorded in [`NodeInfo::addr`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    /// Build a `NodeId` from any string-like input. Leading/trailing
    /// whitespace is trimmed; the result MUST be non-empty.
    pub fn new(id: impl Into<String>) -> Result<Self, ClusterMetaError> {
        let s = id.into().trim().to_string();
        if s.is_empty() {
            return Err(ClusterMetaError::InvalidNodeId(
                "node id must not be empty".into(),
            ));
        }
        if s.contains(char::is_whitespace) {
            return Err(ClusterMetaError::InvalidNodeId(format!(
                "node id {s:?} must not contain whitespace"
            )));
        }
        Ok(Self(s))
    }

    /// Borrow the inner string.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Shard + node state
// ---------------------------------------------------------------------------

/// Lifecycle state of a single shard group.
//
// NOTE: no `#[serde(tag = "...")]` — the adjacently-tagged representation
// relies on `deserialize_any`, which the bincode 1.x wire format does not
// support. Using the default (externally-tagged) form lets both the JSON
// HTTP layer and the bincode Raft wire format roundtrip this enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShardState {
    /// Serving reads and writes normally.
    Active,
    /// Replicas are being added or removed. Reads/writes continue but the
    /// coordinator MUST include the new members in the scatter set so no
    /// committed writes are missed.
    Reconfiguring {
        /// Members being added.
        adding: Vec<NodeId>,
        /// Members being removed.
        removing: Vec<NodeId>,
    },
    /// Shard is offline (no healthy replicas). Writes return
    /// `ERR_SHARD_FAILURE`; metadata stays so the operator can diagnose.
    Offline {
        /// Human-readable reason.
        reason: String,
    },
}

impl ShardState {
    /// True if writes should be accepted in this state.
    #[must_use]
    pub fn is_writable(&self) -> bool {
        matches!(self, Self::Active | Self::Reconfiguring { .. })
    }
}

/// A shard's Raft group + members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardGroup {
    /// Shard id.
    pub shard_id: ShardId,
    /// Raft group members — ordered for deterministic serialization.
    pub members: Vec<NodeId>,
    /// Cached Raft leader. Source of truth is the Raft group; this is a
    /// hint for the coordinator. `None` when the shard has no elected
    /// leader yet.
    pub leader: Option<NodeId>,
    /// Lifecycle state.
    pub state: ShardState,
}

impl ShardGroup {
    /// Construct a freshly-active shard group.
    pub fn new(shard_id: ShardId, members: Vec<NodeId>) -> Self {
        Self {
            shard_id,
            members,
            leader: None,
            state: ShardState::Active,
        }
    }

    /// True iff `node` is a member of this shard.
    #[inline]
    #[must_use]
    pub fn contains(&self, node: &NodeId) -> bool {
        self.members.iter().any(|m| m == node)
    }
}

/// Non-Raft-aware info about a cluster node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Stable node id.
    pub node_id: NodeId,
    /// TCP socket for Raft + coordinator traffic.
    pub addr: SocketAddr,
    /// Region / AZ hint used for rack-aware replica placement. Free-form
    /// string; empty disables rack-awareness for this node.
    #[serde(default)]
    pub zone: String,
}

impl NodeInfo {
    /// Construct a `NodeInfo` with an empty zone.
    pub fn new(node_id: NodeId, addr: SocketAddr) -> Self {
        Self {
            node_id,
            addr,
            zone: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ClusterMeta
// ---------------------------------------------------------------------------

/// Authoritative cluster metadata. This is what the metadata Raft group
/// stores; the pure struct is what Phase 1 tests and what the other
/// sharding modules consume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterMeta {
    /// Cluster-wide UUID, stable for the lifetime of the cluster.
    pub cluster_id: Uuid,
    /// Monotonic counter. Every mutation MUST advance this by 1. Consumers
    /// use it to detect stale caches.
    pub generation: u64,
    /// Total number of data shards. Fixed at cluster bootstrap; changing
    /// it is out of scope for V2 (tracked for V2.1).
    pub num_shards: u32,
    /// Data shards, indexed by `shard_id`. Length MUST equal
    /// `num_shards` after [`validate`].
    pub shards: Vec<ShardGroup>,
    /// Membership of the metadata Raft group (usually every known node).
    pub metadata_members: Vec<NodeId>,
    /// Cached metadata-group leader.
    pub metadata_leader: Option<NodeId>,
    /// Known nodes, keyed by id. `BTreeMap` for deterministic
    /// serialization / hashing.
    pub nodes: BTreeMap<NodeId, NodeInfo>,
}

impl ClusterMeta {
    /// Build an empty cluster meta with a fresh UUID.
    ///
    /// [`validate`] will reject this until at least one node has been
    /// added and `num_shards >= 1`.
    pub fn empty() -> Self {
        Self {
            cluster_id: Uuid::new_v4(),
            generation: 0,
            num_shards: 0,
            shards: Vec::new(),
            metadata_members: Vec::new(),
            metadata_leader: None,
            nodes: BTreeMap::new(),
        }
    }

    /// Bootstrap a cluster with `num_shards` data shards and `replica_factor`
    /// replicas per shard. Replicas are assigned round-robin across
    /// `initial_nodes`, which MUST be non-empty and contain at least
    /// `replica_factor` distinct nodes.
    ///
    /// Every returned [`ShardGroup`] starts at `leader = None` (Raft
    /// election picks the leader on first heartbeat) and
    /// `state = Active`.
    pub fn bootstrap(
        initial_nodes: Vec<NodeInfo>,
        num_shards: u32,
        replica_factor: u32,
    ) -> Result<Self, ClusterMetaError> {
        if initial_nodes.is_empty() {
            return Err(ClusterMetaError::Bootstrap(
                "initial_nodes must be non-empty".into(),
            ));
        }
        if num_shards == 0 {
            return Err(ClusterMetaError::Bootstrap(
                "num_shards must be >= 1".into(),
            ));
        }
        if replica_factor == 0 {
            return Err(ClusterMetaError::Bootstrap(
                "replica_factor must be >= 1".into(),
            ));
        }
        if (replica_factor as usize) > initial_nodes.len() {
            return Err(ClusterMetaError::Bootstrap(format!(
                "replica_factor={replica_factor} exceeds cluster size={}",
                initial_nodes.len()
            )));
        }

        // Reject duplicate node ids up front.
        {
            let mut seen = BTreeSet::new();
            for n in &initial_nodes {
                if !seen.insert(n.node_id.clone()) {
                    return Err(ClusterMetaError::Bootstrap(format!(
                        "duplicate node id {} in initial_nodes",
                        n.node_id
                    )));
                }
            }
        }

        let nodes: BTreeMap<NodeId, NodeInfo> = initial_nodes
            .iter()
            .map(|n| (n.node_id.clone(), n.clone()))
            .collect();

        // Round-robin replica placement: shard i's j-th replica lands on
        // `initial_nodes[(i + j) % len]`. Deterministic and distributes
        // leadership roughly evenly across nodes.
        let n = initial_nodes.len();
        let mut shards = Vec::with_capacity(num_shards as usize);
        for s in 0..num_shards {
            let mut members = Vec::with_capacity(replica_factor as usize);
            for r in 0..replica_factor {
                let idx = (s as usize + r as usize) % n;
                members.push(initial_nodes[idx].node_id.clone());
            }
            shards.push(ShardGroup::new(ShardId::new(s), members));
        }

        let metadata_members: Vec<NodeId> =
            initial_nodes.iter().map(|n| n.node_id.clone()).collect();

        let meta = Self {
            cluster_id: Uuid::new_v4(),
            generation: 1,
            num_shards,
            shards,
            metadata_members,
            metadata_leader: None,
            nodes,
        };
        meta.validate()?;
        Ok(meta)
    }

    /// Invariant check. Called after every mutation; metadata that fails
    /// validation MUST be rejected before it reaches the Raft log.
    pub fn validate(&self) -> Result<(), ClusterMetaError> {
        if self.num_shards == 0 {
            return Err(ClusterMetaError::Invariant(
                "num_shards must be >= 1".into(),
            ));
        }
        if self.shards.len() != self.num_shards as usize {
            return Err(ClusterMetaError::Invariant(format!(
                "shards.len()={} != num_shards={}",
                self.shards.len(),
                self.num_shards
            )));
        }
        for (i, s) in self.shards.iter().enumerate() {
            if s.shard_id.as_u32() as usize != i {
                return Err(ClusterMetaError::Invariant(format!(
                    "shard at index {i} has shard_id={}",
                    s.shard_id
                )));
            }
            if s.members.is_empty() {
                return Err(ClusterMetaError::Invariant(format!(
                    "shard {} has no members",
                    s.shard_id
                )));
            }
            // All shard members must be known nodes.
            for m in &s.members {
                if !self.nodes.contains_key(m) {
                    return Err(ClusterMetaError::Invariant(format!(
                        "shard {} references unknown node {m}",
                        s.shard_id
                    )));
                }
            }
            // Distinct members only.
            let mut seen = BTreeSet::new();
            for m in &s.members {
                if !seen.insert(m) {
                    return Err(ClusterMetaError::Invariant(format!(
                        "shard {} has duplicate member {m}",
                        s.shard_id
                    )));
                }
            }
        }
        // Metadata members must all be known nodes.
        for m in &self.metadata_members {
            if !self.nodes.contains_key(m) {
                return Err(ClusterMetaError::Invariant(format!(
                    "metadata group references unknown node {m}"
                )));
            }
        }
        Ok(())
    }

    /// Apply a [`MetaChange`] in place, validating + bumping the
    /// generation atomically. The operation is transactional: if
    /// validation fails the original state is restored.
    pub fn apply(&mut self, change: MetaChange) -> Result<(), ClusterMetaError> {
        // Apply against a clone so we can roll back on validation failure.
        let mut next = self.clone();
        next.apply_inner(change)?;
        next.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| ClusterMetaError::Invariant("generation overflow".into()))?;
        next.validate()?;
        *self = next;
        Ok(())
    }

    fn apply_inner(&mut self, change: MetaChange) -> Result<(), ClusterMetaError> {
        match change {
            MetaChange::AddNode(info) => {
                if self.nodes.contains_key(&info.node_id) {
                    return Err(ClusterMetaError::Conflict(format!(
                        "node {} already in cluster",
                        info.node_id
                    )));
                }
                self.nodes.insert(info.node_id.clone(), info);
            }
            MetaChange::RemoveNode { node_id } => {
                if !self.nodes.contains_key(&node_id) {
                    return Err(ClusterMetaError::Conflict(format!(
                        "node {node_id} not in cluster"
                    )));
                }
                // Must not be a member of any shard or the metadata group.
                for s in &self.shards {
                    if s.contains(&node_id) {
                        return Err(ClusterMetaError::Conflict(format!(
                            "node {node_id} still a member of {}; move shards off first",
                            s.shard_id
                        )));
                    }
                }
                if self.metadata_members.contains(&node_id) {
                    return Err(ClusterMetaError::Conflict(format!(
                        "node {node_id} still a member of metadata group"
                    )));
                }
                self.nodes.remove(&node_id);
            }
            MetaChange::ReplaceShardMember {
                shard_id,
                remove,
                add,
            } => {
                if !self.nodes.contains_key(&add) {
                    return Err(ClusterMetaError::Conflict(format!(
                        "cannot add {add} to {shard_id}: not a cluster node"
                    )));
                }
                let shard = self
                    .shards
                    .get_mut(shard_id.as_u32() as usize)
                    .ok_or_else(|| {
                        ClusterMetaError::Conflict(format!("unknown shard {shard_id}"))
                    })?;
                let pos = shard
                    .members
                    .iter()
                    .position(|m| m == &remove)
                    .ok_or_else(|| {
                        ClusterMetaError::Conflict(format!(
                            "{remove} is not a member of {shard_id}"
                        ))
                    })?;
                if shard.members.iter().any(|m| m == &add) {
                    return Err(ClusterMetaError::Conflict(format!(
                        "{add} already a member of {shard_id}"
                    )));
                }
                shard.members[pos] = add;
                if shard.leader.as_ref() == Some(&remove) {
                    shard.leader = None;
                }
            }
            MetaChange::UpdateLeader { shard_id, leader } => {
                let shard = self
                    .shards
                    .get_mut(shard_id.as_u32() as usize)
                    .ok_or_else(|| {
                        ClusterMetaError::Conflict(format!("unknown shard {shard_id}"))
                    })?;
                if let Some(ref l) = leader {
                    if !shard.contains(l) {
                        return Err(ClusterMetaError::Conflict(format!(
                            "leader hint {l} not a member of {shard_id}"
                        )));
                    }
                }
                shard.leader = leader;
            }
            MetaChange::SetShardState { shard_id, state } => {
                let shard = self
                    .shards
                    .get_mut(shard_id.as_u32() as usize)
                    .ok_or_else(|| {
                        ClusterMetaError::Conflict(format!("unknown shard {shard_id}"))
                    })?;
                shard.state = state;
            }
        }
        Ok(())
    }

    /// The shard group that owns `shard_id`, or `None` if out of range.
    #[inline]
    #[must_use]
    pub fn shard(&self, shard_id: ShardId) -> Option<&ShardGroup> {
        self.shards.get(shard_id.as_u32() as usize)
    }

    /// Map of `node_id` → list of shards hosted there. `BTreeMap` keeps
    /// iteration order stable for the rebalancer + tests.
    #[must_use]
    pub fn shards_per_node(&self) -> BTreeMap<NodeId, Vec<ShardId>> {
        let mut out: BTreeMap<NodeId, Vec<ShardId>> = BTreeMap::new();
        for s in &self.shards {
            for m in &s.members {
                out.entry(m.clone()).or_default().push(s.shard_id);
            }
        }
        // Fill in nodes that host no shards so the rebalancer sees zero
        // counts explicitly (it needs those to move replicas onto empty
        // nodes).
        for n in self.nodes.keys() {
            out.entry(n.clone()).or_default();
        }
        out
    }

    /// Upper bound the rebalancer tries to enforce: every node should host
    /// at most `ceil((num_shards * replica_factor) / num_nodes)` replicas.
    #[must_use]
    pub fn max_replicas_per_node(&self) -> usize {
        if self.nodes.is_empty() {
            return 0;
        }
        let total: usize = self.shards.iter().map(|s| s.members.len()).sum();
        let n = self.nodes.len();
        total.div_ceil(n)
    }

    /// Count replicas hosted by `node`.
    #[must_use]
    pub fn replicas_on(&self, node: &NodeId) -> usize {
        self.shards.iter().filter(|s| s.contains(node)).count()
    }
}

// ---------------------------------------------------------------------------
// MetaChange — the operations that advance generation
// ---------------------------------------------------------------------------

/// A single mutation to [`ClusterMeta`]. Raft log entries in the metadata
/// group are exactly this enum (bincode-serialized).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetaChange {
    /// Register a new node.
    AddNode(NodeInfo),
    /// Remove a node that no longer hosts any shard or metadata replica.
    RemoveNode { node_id: NodeId },
    /// Replace one shard replica with another. This is the atomic unit
    /// of rebalance + of drain/remove workflows.
    ReplaceShardMember {
        shard_id: ShardId,
        remove: NodeId,
        add: NodeId,
    },
    /// Update the cached leader hint for a shard. Produced by the
    /// shard's Raft group via a metadata notification.
    UpdateLeader {
        shard_id: ShardId,
        leader: Option<NodeId>,
    },
    /// Update lifecycle state for a shard.
    SetShardState {
        shard_id: ShardId,
        state: ShardState,
    },
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors surfaced by cluster-metadata operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ClusterMetaError {
    /// Initial-state construction failed.
    #[error("cluster metadata bootstrap error: {0}")]
    Bootstrap(String),
    /// Invariant violated after a mutation.
    #[error("cluster metadata invariant violated: {0}")]
    Invariant(String),
    /// Mutation conflicts with current state (e.g. removing a node still
    /// hosting a replica).
    #[error("cluster metadata conflict: {0}")]
    Conflict(String),
    /// Invalid node id.
    #[error("invalid node id: {0}")]
    InvalidNodeId(String),
}

// Make the module usable by the crate-wide `Result` without forcing a
// full `From` impl on the top-level `Error` enum yet — the coordinator
// layer translates as needed.
impl From<ClusterMetaError> for crate::Error {
    fn from(err: ClusterMetaError) -> Self {
        crate::Error::internal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, port: u16) -> NodeInfo {
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        NodeInfo::new(NodeId::new(id).unwrap(), addr)
    }

    fn three_node_cluster() -> ClusterMeta {
        ClusterMeta::bootstrap(
            vec![
                node("node-a", 15480),
                node("node-b", 15481),
                node("node-c", 15482),
            ],
            3,
            3,
        )
        .unwrap()
    }

    #[test]
    fn node_id_rejects_empty() {
        assert!(NodeId::new("").is_err());
        assert!(NodeId::new("   ").is_err());
    }

    #[test]
    fn node_id_rejects_whitespace() {
        assert!(NodeId::new("node a").is_err());
        assert!(NodeId::new("node\ta").is_err());
    }

    #[test]
    fn node_id_trims_surrounding_whitespace() {
        let n = NodeId::new("  node-a  ").unwrap();
        assert_eq!(n.as_str(), "node-a");
    }

    #[test]
    fn bootstrap_requires_nodes() {
        let err = ClusterMeta::bootstrap(vec![], 3, 3).unwrap_err();
        assert!(matches!(err, ClusterMetaError::Bootstrap(_)));
    }

    #[test]
    fn bootstrap_requires_num_shards_gt_zero() {
        let err = ClusterMeta::bootstrap(vec![node("node-a", 1)], 0, 1).unwrap_err();
        assert!(matches!(err, ClusterMetaError::Bootstrap(_)));
    }

    #[test]
    fn bootstrap_rejects_replica_factor_over_cluster_size() {
        let err = ClusterMeta::bootstrap(vec![node("node-a", 1)], 1, 2).unwrap_err();
        assert!(matches!(err, ClusterMetaError::Bootstrap(_)));
    }

    #[test]
    fn bootstrap_rejects_duplicate_node_ids() {
        let err =
            ClusterMeta::bootstrap(vec![node("node-a", 1), node("node-a", 2)], 1, 1).unwrap_err();
        assert!(matches!(err, ClusterMetaError::Bootstrap(_)));
    }

    #[test]
    fn bootstrap_produces_valid_metadata() {
        let meta = three_node_cluster();
        assert_eq!(meta.num_shards, 3);
        assert_eq!(meta.shards.len(), 3);
        assert_eq!(meta.generation, 1);
        for (i, s) in meta.shards.iter().enumerate() {
            assert_eq!(s.shard_id.as_u32() as usize, i);
            assert_eq!(s.members.len(), 3);
            assert!(matches!(s.state, ShardState::Active));
            assert!(s.leader.is_none());
        }
    }

    #[test]
    fn round_robin_membership_rotates() {
        let meta = three_node_cluster();
        // With 3 shards × 3 replicas on 3 nodes, every node is a member of
        // every shard (complete replication). That's expected when
        // replica_factor == num_nodes.
        for s in &meta.shards {
            assert_eq!(s.members.len(), 3);
        }
    }

    #[test]
    fn validate_detects_shard_index_mismatch() {
        let mut meta = three_node_cluster();
        meta.shards[1].shard_id = ShardId::new(99);
        assert!(matches!(
            meta.validate(),
            Err(ClusterMetaError::Invariant(_))
        ));
    }

    #[test]
    fn validate_detects_unknown_member() {
        let mut meta = three_node_cluster();
        meta.shards[0].members[0] = NodeId::new("ghost").unwrap();
        assert!(matches!(
            meta.validate(),
            Err(ClusterMetaError::Invariant(_))
        ));
    }

    #[test]
    fn validate_detects_duplicate_member() {
        let mut meta = three_node_cluster();
        meta.shards[0].members[1] = meta.shards[0].members[0].clone();
        assert!(matches!(
            meta.validate(),
            Err(ClusterMetaError::Invariant(_))
        ));
    }

    #[test]
    fn apply_bumps_generation() {
        let mut meta = three_node_cluster();
        let gen0 = meta.generation;
        meta.apply(MetaChange::UpdateLeader {
            shard_id: ShardId::new(0),
            leader: Some(NodeId::new("node-a").unwrap()),
        })
        .unwrap();
        assert_eq!(meta.generation, gen0 + 1);
        assert_eq!(
            meta.shard(ShardId::new(0))
                .unwrap()
                .leader
                .as_ref()
                .unwrap()
                .as_str(),
            "node-a"
        );
    }

    #[test]
    fn apply_rejects_leader_not_in_shard() {
        let mut meta = three_node_cluster();
        let err = meta
            .apply(MetaChange::UpdateLeader {
                shard_id: ShardId::new(0),
                leader: Some(NodeId::new("ghost").unwrap()),
            })
            .unwrap_err();
        assert!(matches!(err, ClusterMetaError::Conflict(_)));
        assert_eq!(meta.generation, 1, "rejection must not advance generation");
    }

    #[test]
    fn apply_rejects_duplicate_add_node() {
        let mut meta = three_node_cluster();
        let err = meta
            .apply(MetaChange::AddNode(node("node-a", 9999)))
            .unwrap_err();
        assert!(matches!(err, ClusterMetaError::Conflict(_)));
    }

    #[test]
    fn apply_add_node_succeeds_when_new() {
        let mut meta = three_node_cluster();
        meta.apply(MetaChange::AddNode(node("node-d", 9999)))
            .unwrap();
        assert!(meta.nodes.contains_key(&NodeId::new("node-d").unwrap()));
    }

    #[test]
    fn apply_remove_node_blocked_while_hosting_shard() {
        let mut meta = three_node_cluster();
        // node-a hosts every shard in this config, so removal must fail.
        let err = meta
            .apply(MetaChange::RemoveNode {
                node_id: NodeId::new("node-a").unwrap(),
            })
            .unwrap_err();
        assert!(matches!(err, ClusterMetaError::Conflict(_)));
    }

    #[test]
    fn apply_replace_member_works() {
        let mut meta = three_node_cluster();
        // Add a 4th node, then swap it in for node-a on shard 0.
        meta.apply(MetaChange::AddNode(node("node-d", 15483)))
            .unwrap();
        meta.apply(MetaChange::ReplaceShardMember {
            shard_id: ShardId::new(0),
            remove: NodeId::new("node-a").unwrap(),
            add: NodeId::new("node-d").unwrap(),
        })
        .unwrap();
        let s0 = meta.shard(ShardId::new(0)).unwrap();
        assert!(s0.contains(&NodeId::new("node-d").unwrap()));
        assert!(!s0.contains(&NodeId::new("node-a").unwrap()));
    }

    #[test]
    fn apply_replace_rejects_unknown_target() {
        let mut meta = three_node_cluster();
        let err = meta
            .apply(MetaChange::ReplaceShardMember {
                shard_id: ShardId::new(0),
                remove: NodeId::new("node-a").unwrap(),
                add: NodeId::new("node-x").unwrap(),
            })
            .unwrap_err();
        assert!(matches!(err, ClusterMetaError::Conflict(_)));
    }

    #[test]
    fn apply_replace_clears_leader_hint_if_leader_removed() {
        let mut meta = three_node_cluster();
        meta.apply(MetaChange::AddNode(node("node-d", 15483)))
            .unwrap();
        meta.apply(MetaChange::UpdateLeader {
            shard_id: ShardId::new(0),
            leader: Some(NodeId::new("node-a").unwrap()),
        })
        .unwrap();
        meta.apply(MetaChange::ReplaceShardMember {
            shard_id: ShardId::new(0),
            remove: NodeId::new("node-a").unwrap(),
            add: NodeId::new("node-d").unwrap(),
        })
        .unwrap();
        assert!(meta.shard(ShardId::new(0)).unwrap().leader.is_none());
    }

    #[test]
    fn apply_rollback_on_invariant_violation() {
        let mut meta = three_node_cluster();
        let snapshot = meta.clone();
        // Removing the only member of a shard would fail validation.
        // Construct such a change by replacing the only valid target.
        let err = meta
            .apply(MetaChange::ReplaceShardMember {
                shard_id: ShardId::new(0),
                remove: NodeId::new("node-a").unwrap(),
                add: NodeId::new("node-b").unwrap(), // already a member → conflict
            })
            .unwrap_err();
        assert!(matches!(err, ClusterMetaError::Conflict(_)));
        assert_eq!(meta, snapshot, "state must be restored on rejection");
    }

    #[test]
    fn shards_per_node_lists_every_node() {
        let meta = three_node_cluster();
        let map = meta.shards_per_node();
        for n in ["node-a", "node-b", "node-c"] {
            let nid = NodeId::new(n).unwrap();
            assert!(
                map.contains_key(&nid),
                "node {n} missing from shards_per_node"
            );
            assert_eq!(map[&nid].len(), 3); // all three shards
        }
    }

    #[test]
    fn max_replicas_per_node_is_ceiling() {
        let meta = three_node_cluster();
        // 3 shards × 3 replicas = 9 / 3 nodes = 3.
        assert_eq!(meta.max_replicas_per_node(), 3);
    }

    #[test]
    fn shard_state_writable_predicate() {
        assert!(ShardState::Active.is_writable());
        assert!(
            ShardState::Reconfiguring {
                adding: vec![],
                removing: vec![]
            }
            .is_writable()
        );
        assert!(
            !ShardState::Offline {
                reason: "test".into()
            }
            .is_writable()
        );
    }

    #[test]
    fn set_shard_state_applied() {
        let mut meta = three_node_cluster();
        meta.apply(MetaChange::SetShardState {
            shard_id: ShardId::new(1),
            state: ShardState::Offline {
                reason: "disk full".into(),
            },
        })
        .unwrap();
        assert!(!meta.shard(ShardId::new(1)).unwrap().state.is_writable());
    }

    #[test]
    fn empty_meta_fails_validation() {
        let meta = ClusterMeta::empty();
        assert!(matches!(
            meta.validate(),
            Err(ClusterMetaError::Invariant(_))
        ));
    }

    #[test]
    fn apply_preserves_invariants_across_many_changes() {
        let mut meta = three_node_cluster();
        meta.apply(MetaChange::AddNode(node("node-d", 15483)))
            .unwrap();
        meta.apply(MetaChange::AddNode(node("node-e", 15484)))
            .unwrap();
        meta.apply(MetaChange::ReplaceShardMember {
            shard_id: ShardId::new(0),
            remove: NodeId::new("node-a").unwrap(),
            add: NodeId::new("node-d").unwrap(),
        })
        .unwrap();
        meta.apply(MetaChange::ReplaceShardMember {
            shard_id: ShardId::new(2),
            remove: NodeId::new("node-c").unwrap(),
            add: NodeId::new("node-e").unwrap(),
        })
        .unwrap();
        assert!(meta.validate().is_ok());
        assert_eq!(meta.generation, 5);
    }

    #[test]
    fn shard_display_format() {
        assert_eq!(ShardId::new(7).to_string(), "shard-7");
    }

    #[test]
    fn cluster_meta_roundtrips_through_bincode() {
        let meta = three_node_cluster();
        let bytes = bincode::serialize(&meta).unwrap();
        let back: ClusterMeta = bincode::deserialize(&bytes).unwrap();
        assert_eq!(meta, back);
    }

    #[test]
    fn hash_map_of_node_id_works() {
        let mut m: HashMap<NodeId, u32> = HashMap::new();
        m.insert(NodeId::new("a").unwrap(), 1);
        m.insert(NodeId::new("b").unwrap(), 2);
        assert_eq!(m.get(&NodeId::new("a").unwrap()), Some(&1));
    }
}
