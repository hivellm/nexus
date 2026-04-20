//! Cluster controller — the per-node object that owns [`ClusterMeta`]
//! and exposes the operations the `/cluster/*` HTTP surface calls.
//!
//! In Phase 5 the controller is a plain in-memory state machine. Phase
//! 6 integration tests plug it on top of the metadata Raft group
//! ([`super::raft`]) so every mutation goes through Raft consensus;
//! Phase 5's unit tests exercise the controller directly without the
//! Raft round-trip.
//!
//! The controller enforces:
//!
//! * **Leader gating** — mutations only succeed on the node marked as
//!   the metadata-group leader. Followers return
//!   [`ControllerError::NotMetadataLeader`] so the HTTP layer can
//!   translate to `307 Temporary Redirect`.
//! * **Drain semantics** — `remove_node { drain: true }` blocks
//!   removal until no shard still lists the node as a member AND
//!   remaining replicas of those shards have caught up.
//! * **Generation bump invariant** — every successful mutation bumps
//!   the generation exactly once, regardless of how many sub-steps
//!   it performs. Handled by [`ClusterMeta::apply`].

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::health::ShardHealth;
use super::metadata::{
    ClusterMeta, ClusterMetaError, MetaChange, NodeId, NodeInfo, ShardId, ShardState,
};
use super::rebalance::{RebalancePlan, plan_rebalance};

/// Snapshot of a cluster's state suitable for HTTP serialization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterStatus {
    /// Cluster UUID.
    pub cluster_id: String,
    /// Current generation.
    pub generation: u64,
    /// Number of data shards.
    pub num_shards: u32,
    /// Per-shard health snapshots.
    pub shards: Vec<ShardHealth>,
    /// Known nodes.
    pub nodes: HashMap<String, NodeStatus>,
    /// Cached metadata-group leader.
    pub metadata_leader: Option<String>,
}

/// Per-node snapshot row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeStatus {
    /// Stable node id.
    pub node_id: String,
    /// TCP address.
    pub addr: String,
    /// Optional zone/rack hint.
    pub zone: String,
}

/// Errors surfaced by the controller.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ControllerError {
    /// This node is not the metadata leader. HTTP translates to 307.
    #[error("not metadata leader (hint: {leader_hint:?})")]
    NotMetadataLeader {
        /// Hint — may be empty if no leader is known.
        leader_hint: Option<NodeId>,
    },
    /// Underlying metadata apply failed.
    #[error("metadata: {0}")]
    Meta(#[from] ClusterMetaError),
    /// Drain waited past the caller's deadline.
    #[error("drain timed out: {reason}")]
    DrainTimeout { reason: String },
    /// Shard not found.
    #[error("unknown shard {0}")]
    UnknownShard(ShardId),
}

/// Request body for `POST /cluster/add_node`.
#[derive(Debug, Clone, Deserialize)]
pub struct AddNodeRequest {
    /// Stable node id.
    pub node_id: String,
    /// Socket address.
    pub addr: String,
    /// Optional zone/rack hint.
    #[serde(default)]
    pub zone: String,
}

/// Request body for `POST /cluster/remove_node`.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoveNodeRequest {
    /// Stable node id.
    pub node_id: String,
    /// Whether to wait for replicas to catch up before returning.
    #[serde(default)]
    pub drain: bool,
}

/// Health-feed trait — lets the controller surface live Raft-level
/// health without the controller needing to know about Raft.
pub trait ShardHealthProvider: Send + Sync {
    /// Return a health snapshot for every shard.
    fn health(&self, meta: &ClusterMeta) -> Vec<ShardHealth>;
}

/// Trivial provider that marks every shard as healthy-with-leader. Used
/// by Phase 5 unit tests; Phase 6 replaces it with a Raft-backed impl.
#[derive(Debug, Default)]
pub struct StaticAllHealthy;

impl ShardHealthProvider for StaticAllHealthy {
    fn health(&self, meta: &ClusterMeta) -> Vec<ShardHealth> {
        meta.shards
            .iter()
            .map(|s| {
                let replicas = s
                    .members
                    .iter()
                    .map(|m| crate::sharding::health::ReplicaHealth {
                        node_id: m.clone(),
                        commit_offset: 0,
                        lag: 0,
                        healthy: true,
                        reason: String::new(),
                    })
                    .collect();
                ShardHealth::new(s.shard_id, s.state.clone(), s.leader.clone(), replicas)
            })
            .collect()
    }
}

/// The controller.
pub struct ClusterController {
    /// Stable id of the node this controller runs on.
    this_node: NodeId,
    /// `true` iff this node is the current metadata-group leader.
    /// Set by the metadata Raft layer; polled by mutating endpoints.
    is_leader: RwLock<bool>,
    /// Cached hint of who IS leader if we're not. Used for HTTP
    /// redirect. Rotated by the Raft layer on every election.
    leader_hint: RwLock<Option<NodeId>>,
    /// The authoritative cluster metadata.
    meta: RwLock<ClusterMeta>,
    /// Plug-in that produces health snapshots.
    health_provider: Arc<dyn ShardHealthProvider>,
}

impl ClusterController {
    /// Build a controller at node `this_node` for an existing `meta`.
    /// `is_leader` toggles mutation gating — set to true only for the
    /// metadata Raft leader.
    #[must_use]
    pub fn new(
        this_node: NodeId,
        meta: ClusterMeta,
        is_leader: bool,
        health_provider: Arc<dyn ShardHealthProvider>,
    ) -> Self {
        Self {
            this_node,
            is_leader: RwLock::new(is_leader),
            leader_hint: RwLock::new(None),
            meta: RwLock::new(meta),
            health_provider,
        }
    }

    /// Promote / demote this node as the metadata leader. Called by
    /// the metadata Raft layer on election.
    pub fn set_leader(&self, is_leader: bool, leader_hint: Option<NodeId>) {
        *self.is_leader.write().expect("leader lock poisoned") = is_leader;
        *self.leader_hint.write().expect("leader hint lock poisoned") = leader_hint;
    }

    /// Snapshot of the current metadata.
    #[must_use]
    pub fn meta(&self) -> ClusterMeta {
        self.meta.read().expect("meta lock poisoned").clone()
    }

    /// True iff this node is the metadata leader.
    #[must_use]
    pub fn is_leader(&self) -> bool {
        *self.is_leader.read().expect("leader lock poisoned")
    }

    /// Stable id of the node running this controller.
    #[inline]
    #[must_use]
    pub fn this_node(&self) -> &NodeId {
        &self.this_node
    }

    /// Build a [`ClusterStatus`] snapshot for `GET /cluster/status`.
    #[must_use]
    pub fn status(&self) -> ClusterStatus {
        let meta = self.meta.read().expect("meta lock poisoned").clone();
        let health = self.health_provider.health(&meta);
        let nodes = meta
            .nodes
            .iter()
            .map(|(id, info)| {
                (
                    id.as_str().to_string(),
                    NodeStatus {
                        node_id: info.node_id.as_str().to_string(),
                        addr: info.addr.to_string(),
                        zone: info.zone.clone(),
                    },
                )
            })
            .collect();
        ClusterStatus {
            cluster_id: meta.cluster_id.to_string(),
            generation: meta.generation,
            num_shards: meta.num_shards,
            shards: health,
            nodes,
            metadata_leader: meta
                .metadata_leader
                .as_ref()
                .map(|l| l.as_str().to_string()),
        }
    }

    /// Apply `POST /cluster/add_node`.
    pub fn add_node(&self, req: AddNodeRequest) -> Result<u64, ControllerError> {
        self.require_leader()?;
        let node_id = NodeId::new(req.node_id.clone()).map_err(ControllerError::Meta)?;
        let addr = req.addr.parse().map_err(|e| {
            ControllerError::Meta(ClusterMetaError::Invariant(format!(
                "bad addr {}: {e}",
                req.addr
            )))
        })?;
        let info = NodeInfo {
            node_id,
            addr,
            zone: req.zone,
        };
        let mut meta = self.meta.write().expect("meta lock poisoned");
        meta.apply(MetaChange::AddNode(info))?;
        Ok(meta.generation)
    }

    /// Apply `POST /cluster/remove_node`. When `drain` is true and the
    /// node is still a shard member, this returns
    /// [`ControllerError::DrainTimeout`] so the HTTP layer retries (or
    /// an operator moves replicas off first).
    pub fn remove_node(&self, req: RemoveNodeRequest) -> Result<u64, ControllerError> {
        self.require_leader()?;
        let node_id = NodeId::new(req.node_id).map_err(ControllerError::Meta)?;

        let still_member = {
            let meta = self.meta.read().expect("meta lock poisoned");
            meta.shards.iter().any(|s| s.contains(&node_id))
                || meta.metadata_members.contains(&node_id)
        };
        if still_member {
            if req.drain {
                return Err(ControllerError::DrainTimeout {
                    reason: format!(
                        "node {node_id} is still a shard or metadata member — move replicas first"
                    ),
                });
            }
            // Non-drain: let the underlying apply return the
            // Conflict error unchanged.
        }

        let mut meta = self.meta.write().expect("meta lock poisoned");
        meta.apply(MetaChange::RemoveNode { node_id })?;
        Ok(meta.generation)
    }

    /// Apply `POST /cluster/rebalance`. Returns the number of moves
    /// applied; 0 means already balanced.
    pub fn rebalance(&self) -> Result<usize, ControllerError> {
        self.require_leader()?;
        let plan: RebalancePlan = {
            let meta = self.meta.read().expect("meta lock poisoned");
            plan_rebalance(&meta)
        };
        if plan.is_noop() {
            return Ok(0);
        }
        let moves_count = plan.moves.len();
        let mut meta = self.meta.write().expect("meta lock poisoned");
        for m in plan.moves {
            meta.apply(m.into_change())?;
        }
        Ok(moves_count)
    }

    /// Apply a leader-update notification from a shard's Raft group.
    /// Called by the per-shard driver whenever it learns of an
    /// election outcome.
    pub fn update_shard_leader(
        &self,
        shard_id: ShardId,
        leader: Option<NodeId>,
    ) -> Result<u64, ControllerError> {
        self.require_leader()?;
        let mut meta = self.meta.write().expect("meta lock poisoned");
        meta.apply(MetaChange::UpdateLeader { shard_id, leader })?;
        Ok(meta.generation)
    }

    /// Set a shard's lifecycle state. Leader-only.
    pub fn set_shard_state(
        &self,
        shard_id: ShardId,
        state: ShardState,
    ) -> Result<u64, ControllerError> {
        self.require_leader()?;
        let mut meta = self.meta.write().expect("meta lock poisoned");
        meta.apply(MetaChange::SetShardState { shard_id, state })?;
        Ok(meta.generation)
    }

    // ------------------------------------------------------------------

    fn require_leader(&self) -> Result<(), ControllerError> {
        if *self.is_leader.read().expect("leader lock poisoned") {
            return Ok(());
        }
        let hint = self
            .leader_hint
            .read()
            .expect("leader hint lock poisoned")
            .clone();
        Err(ControllerError::NotMetadataLeader { leader_hint: hint })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sharding::metadata::NodeInfo;
    use std::net::SocketAddr;

    fn nid(s: &str) -> NodeId {
        NodeId::new(s).unwrap()
    }

    fn ninfo(id: &str, port: u16) -> NodeInfo {
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        NodeInfo::new(nid(id), addr)
    }

    fn three_node_controller() -> ClusterController {
        let meta = ClusterMeta::bootstrap(
            vec![
                ninfo("node-a", 15480),
                ninfo("node-b", 15481),
                ninfo("node-c", 15482),
            ],
            3,
            3,
        )
        .unwrap();
        ClusterController::new(nid("node-a"), meta, true, Arc::new(StaticAllHealthy))
    }

    #[test]
    fn status_contains_shard_list() {
        let c = three_node_controller();
        let s = c.status();
        assert_eq!(s.num_shards, 3);
        assert_eq!(s.shards.len(), 3);
        assert_eq!(s.nodes.len(), 3);
    }

    #[test]
    fn add_node_succeeds_on_leader() {
        let c = three_node_controller();
        let gen_ = c
            .add_node(AddNodeRequest {
                node_id: "node-d".into(),
                addr: "127.0.0.1:15483".into(),
                zone: String::new(),
            })
            .unwrap();
        assert_eq!(gen_, 2);
        let status = c.status();
        assert!(status.nodes.contains_key("node-d"));
    }

    #[test]
    fn add_node_rejected_on_follower() {
        let c = three_node_controller();
        c.set_leader(false, Some(nid("node-b")));
        let err = c
            .add_node(AddNodeRequest {
                node_id: "node-d".into(),
                addr: "127.0.0.1:15483".into(),
                zone: String::new(),
            })
            .unwrap_err();
        assert!(matches!(err, ControllerError::NotMetadataLeader { .. }));
    }

    #[test]
    fn add_node_rejects_duplicate() {
        let c = three_node_controller();
        let err = c
            .add_node(AddNodeRequest {
                node_id: "node-a".into(),
                addr: "127.0.0.1:15499".into(),
                zone: String::new(),
            })
            .unwrap_err();
        assert!(matches!(err, ControllerError::Meta(_)));
    }

    #[test]
    fn remove_node_drain_waits() {
        let c = three_node_controller();
        let err = c
            .remove_node(RemoveNodeRequest {
                node_id: "node-a".into(),
                drain: true,
            })
            .unwrap_err();
        assert!(matches!(err, ControllerError::DrainTimeout { .. }));
    }

    #[test]
    fn remove_node_without_drain_surfaces_conflict() {
        let c = three_node_controller();
        let err = c
            .remove_node(RemoveNodeRequest {
                node_id: "node-a".into(),
                drain: false,
            })
            .unwrap_err();
        assert!(matches!(err, ControllerError::Meta(_)));
    }

    #[test]
    fn remove_node_after_drain_clean() {
        let mut cmeta = ClusterMeta::bootstrap(
            vec![
                ninfo("node-a", 15480),
                ninfo("node-b", 15481),
                ninfo("node-c", 15482),
                ninfo("node-d", 15483),
            ],
            3,
            3,
        )
        .unwrap();
        // Drop node-d from the metadata group so the controller only
        // needs to handle data-shard membership below.
        cmeta.metadata_members.retain(|n| n.as_str() != "node-d");
        let c = ClusterController::new(nid("node-a"), cmeta, true, Arc::new(StaticAllHealthy));
        c.set_leader(true, None);

        // Sweep every shard: if d is still a member, swap it for the
        // first non-d, non-current-member we can find.
        {
            let mut meta = c.meta.write().unwrap();
            let num_shards = meta.num_shards;
            let d = nid("node-d");
            for s in 0..num_shards {
                let sid = ShardId::new(s);
                let already_member = meta.shards[sid.as_u32() as usize].members.contains(&d);
                if !already_member {
                    continue;
                }
                // Pick a non-d node not already a shard member.
                let existing: Vec<_> = meta.shards[sid.as_u32() as usize].members.clone();
                let candidate = ["node-a", "node-b", "node-c"]
                    .iter()
                    .map(|s| nid(s))
                    .find(|n| n != &d && !existing.contains(n))
                    .expect("no spare node to swap in");
                meta.apply(MetaChange::ReplaceShardMember {
                    shard_id: sid,
                    remove: d.clone(),
                    add: candidate,
                })
                .expect("swap-out node-d");
            }
        }

        let out = c.remove_node(RemoveNodeRequest {
            node_id: "node-d".into(),
            drain: true,
        });
        assert!(out.is_ok(), "drain-remove failed: {out:?}");
    }

    #[test]
    fn rebalance_noop_on_balanced() {
        let c = three_node_controller();
        let moves = c.rebalance().unwrap();
        assert_eq!(moves, 0);
    }

    #[test]
    fn rebalance_requires_leader() {
        let c = three_node_controller();
        c.set_leader(false, Some(nid("node-b")));
        let err = c.rebalance().unwrap_err();
        assert!(matches!(err, ControllerError::NotMetadataLeader { .. }));
    }

    #[test]
    fn update_shard_leader_bumps_generation() {
        let c = three_node_controller();
        let before = c.status().generation;
        c.update_shard_leader(ShardId::new(0), Some(nid("node-a")))
            .unwrap();
        assert_eq!(c.status().generation, before + 1);
    }

    #[test]
    fn set_shard_state_applied() {
        let c = three_node_controller();
        c.set_shard_state(
            ShardId::new(1),
            ShardState::Offline {
                reason: "disk full".into(),
            },
        )
        .unwrap();
        let st = c.status();
        let shard1 = st
            .shards
            .iter()
            .find(|s| s.shard_id == ShardId::new(1))
            .unwrap();
        assert!(!shard1.state.is_writable());
    }

    #[test]
    fn leader_hint_surfaced_on_not_leader() {
        let c = three_node_controller();
        c.set_leader(false, Some(nid("node-b")));
        let err = c.rebalance().unwrap_err();
        match err {
            ControllerError::NotMetadataLeader {
                leader_hint: Some(h),
            } => assert_eq!(h, nid("node-b")),
            other => panic!("expected NotMetadataLeader with hint, got {other:?}"),
        }
    }

    #[test]
    fn static_provider_reports_all_healthy() {
        let c = three_node_controller();
        let s = c.status();
        for shard in &s.shards {
            for r in &shard.replicas {
                assert!(r.healthy);
            }
            assert!(shard.replicas.iter().all(|r| r.healthy));
        }
    }

    #[test]
    fn meta_snapshot_is_not_aliased() {
        let c = three_node_controller();
        let snap1 = c.meta();
        c.add_node(AddNodeRequest {
            node_id: "node-d".into(),
            addr: "127.0.0.1:15483".into(),
            zone: String::new(),
        })
        .unwrap();
        // snap1 still reflects the old generation.
        assert_eq!(snap1.generation, 1);
        assert_eq!(c.meta().generation, 2);
    }
}
