//! Per-shard / per-replica health snapshots.
//!
//! Produced by the Raft layer on every heartbeat tick and surfaced by
//! `GET /cluster/status`. The model is deliberately a plain data value —
//! reading health is read-only, writing it is the job of the Raft apply
//! loop.

use serde::{Deserialize, Serialize};

use super::metadata::{NodeId, ShardId, ShardState};

/// Lag threshold (log entries) above which a replica is reported unhealthy
/// even though its Raft session is alive. Matches the existing
/// [`crate::replication::LAG_WARNING_THRESHOLD`] so operators do not have
/// to remember two thresholds.
pub const LAG_UNHEALTHY_THRESHOLD: u64 = 10_000;

/// Snapshot of a single Raft replica's progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaHealth {
    /// Node this replica lives on.
    pub node_id: NodeId,
    /// Last log offset this replica has committed.
    pub commit_offset: u64,
    /// Entries behind the leader.
    pub lag: u64,
    /// True iff the replica is reachable AND lag < LAG_UNHEALTHY_THRESHOLD.
    pub healthy: bool,
    /// Free-form reason for unhealthiness; empty when `healthy`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

impl ReplicaHealth {
    /// Build a replica-health snapshot from raw inputs, deriving
    /// `healthy` from the `reachable` flag + `lag`.
    #[must_use]
    pub fn evaluate(node_id: NodeId, commit_offset: u64, lag: u64, reachable: bool) -> Self {
        let (healthy, reason) = if !reachable {
            (false, "unreachable".to_string())
        } else if lag >= LAG_UNHEALTHY_THRESHOLD {
            (false, format!("lag {lag} >= {LAG_UNHEALTHY_THRESHOLD}"))
        } else {
            (true, String::new())
        };
        Self {
            node_id,
            commit_offset,
            lag,
            healthy,
            reason,
        }
    }
}

/// Snapshot of a whole shard group's health.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardHealth {
    /// Shard id.
    pub shard_id: ShardId,
    /// Current lifecycle state (mirrors the metadata group).
    pub state: ShardState,
    /// Current leader, if any.
    pub leader: Option<NodeId>,
    /// Per-replica rows.
    pub replicas: Vec<ReplicaHealth>,
    /// Leader's commit offset (source of truth for lag calculations).
    pub last_commit_offset: u64,
}

impl ShardHealth {
    /// Build a shard-level health view from per-replica rows. The
    /// replicas do NOT need to be sorted; this constructor does not
    /// mutate them.
    #[must_use]
    pub fn new(
        shard_id: ShardId,
        state: ShardState,
        leader: Option<NodeId>,
        replicas: Vec<ReplicaHealth>,
    ) -> Self {
        let last_commit_offset = replicas.iter().map(|r| r.commit_offset).max().unwrap_or(0);
        Self {
            shard_id,
            state,
            leader,
            replicas,
            last_commit_offset,
        }
    }

    /// Count of healthy replicas.
    #[inline]
    #[must_use]
    pub fn healthy_replicas(&self) -> usize {
        self.replicas.iter().filter(|r| r.healthy).count()
    }

    /// True iff there's an elected leader, the state is writable, and
    /// a Raft majority of replicas are healthy.
    #[must_use]
    pub fn is_available(&self) -> bool {
        if !self.state.is_writable() {
            return false;
        }
        if self.leader.is_none() {
            return false;
        }
        let total = self.replicas.len();
        if total == 0 {
            return false;
        }
        let majority = total / 2 + 1;
        self.healthy_replicas() >= majority
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(s: &str) -> NodeId {
        NodeId::new(s).unwrap()
    }

    #[test]
    fn healthy_replica_under_threshold() {
        let r = ReplicaHealth::evaluate(nid("a"), 100, 10, true);
        assert!(r.healthy);
        assert!(r.reason.is_empty());
    }

    #[test]
    fn unreachable_replica_is_unhealthy() {
        let r = ReplicaHealth::evaluate(nid("a"), 100, 0, false);
        assert!(!r.healthy);
        assert_eq!(r.reason, "unreachable");
    }

    #[test]
    fn lag_over_threshold_is_unhealthy() {
        let r = ReplicaHealth::evaluate(nid("a"), 100, LAG_UNHEALTHY_THRESHOLD + 1, true);
        assert!(!r.healthy);
        assert!(r.reason.contains("lag"));
    }

    #[test]
    fn exactly_threshold_is_unhealthy() {
        // Spec says `>= LAG_UNHEALTHY_THRESHOLD` counts as unhealthy.
        let r = ReplicaHealth::evaluate(nid("a"), 100, LAG_UNHEALTHY_THRESHOLD, true);
        assert!(!r.healthy);
    }

    #[test]
    fn shard_health_counts_healthy() {
        let h = ShardHealth::new(
            ShardId::new(0),
            ShardState::Active,
            Some(nid("a")),
            vec![
                ReplicaHealth::evaluate(nid("a"), 100, 0, true),
                ReplicaHealth::evaluate(nid("b"), 100, 0, true),
                ReplicaHealth::evaluate(nid("c"), 0, 0, false),
            ],
        );
        assert_eq!(h.healthy_replicas(), 2);
        assert!(h.is_available());
    }

    #[test]
    fn shard_unavailable_when_no_leader() {
        let h = ShardHealth::new(
            ShardId::new(0),
            ShardState::Active,
            None,
            vec![ReplicaHealth::evaluate(nid("a"), 100, 0, true)],
        );
        assert!(!h.is_available());
    }

    #[test]
    fn shard_unavailable_when_offline() {
        let h = ShardHealth::new(
            ShardId::new(0),
            ShardState::Offline {
                reason: "test".into(),
            },
            Some(nid("a")),
            vec![ReplicaHealth::evaluate(nid("a"), 100, 0, true)],
        );
        assert!(!h.is_available());
    }

    #[test]
    fn shard_unavailable_without_majority() {
        let h = ShardHealth::new(
            ShardId::new(0),
            ShardState::Active,
            Some(nid("a")),
            vec![
                ReplicaHealth::evaluate(nid("a"), 100, 0, true),
                ReplicaHealth::evaluate(nid("b"), 0, 0, false),
                ReplicaHealth::evaluate(nid("c"), 0, 0, false),
            ],
        );
        assert_eq!(h.healthy_replicas(), 1);
        assert!(!h.is_available());
    }

    #[test]
    fn last_commit_offset_is_max() {
        let h = ShardHealth::new(
            ShardId::new(0),
            ShardState::Active,
            Some(nid("a")),
            vec![
                ReplicaHealth::evaluate(nid("a"), 12345, 0, true),
                ReplicaHealth::evaluate(nid("b"), 12000, 345, true),
            ],
        );
        assert_eq!(h.last_commit_offset, 12345);
    }

    #[test]
    fn empty_replica_list_not_available() {
        let h = ShardHealth::new(ShardId::new(0), ShardState::Active, Some(nid("a")), vec![]);
        assert!(!h.is_available());
    }
}
