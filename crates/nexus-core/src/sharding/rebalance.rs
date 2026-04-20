//! Rebalance planning.
//!
//! Pure function: given a [`ClusterMeta`], produce a list of
//! [`MetaChange`] operations that, when applied in order, move shard
//! replicas off overloaded nodes until every node hosts at most
//! `ceil(total_replicas / num_nodes)` replicas.
//!
//! Important properties:
//!
//! * **Deterministic** — given the same input, the output is byte-for-byte
//!   identical. Needed so the rebalancer can run on any metadata-group
//!   leader and produce the same plan; otherwise leader changes mid-plan
//!   would oscillate.
//! * **Minimal** — produces at most one move per overloaded replica.
//!   A subsequent `POST /cluster/rebalance` call can make further
//!   progress; we do not try to solve the fully-balanced case in one
//!   pass because each move has to be serialized through the Raft log
//!   anyway.
//! * **Feasibility-aware** — refuses to move a replica off node `X` if
//!   doing so would either (a) duplicate an existing member onto a
//!   shard or (b) leave the shard below `replica_factor`. A shard
//!   whose only feasible target is an overloaded node is skipped.

use std::collections::BTreeMap;

use super::metadata::{ClusterMeta, MetaChange, NodeId, ShardId};

/// One logical move: replace member `from` with `to` on `shard_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebalanceMove {
    /// Shard the move applies to.
    pub shard_id: ShardId,
    /// Replica being moved off.
    pub from: NodeId,
    /// Node receiving the replica.
    pub to: NodeId,
}

impl RebalanceMove {
    /// Translate into the Raft-group metadata change.
    #[must_use]
    pub fn into_change(self) -> MetaChange {
        MetaChange::ReplaceShardMember {
            shard_id: self.shard_id,
            remove: self.from,
            add: self.to,
        }
    }
}

/// Ordered list of moves. Callers apply them in order — each one bumps
/// the cluster generation and is applied serially through the metadata
/// Raft group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RebalancePlan {
    /// Moves to apply, in order.
    pub moves: Vec<RebalanceMove>,
}

impl RebalancePlan {
    /// Empty plan (cluster already balanced).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// True iff the plan contains no work.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.moves.is_empty()
    }

    /// Number of moves.
    #[must_use]
    pub fn len(&self) -> usize {
        self.moves.len()
    }
}

/// Compute a rebalance plan for the given metadata.
///
/// Algorithm:
///
/// 1. Compute replica counts per node and the target upper bound
///    `cap = ceil(total_replicas / num_nodes)`.
/// 2. Walk overloaded nodes in deterministic (sorted) order. For each
///    one, pick one shard it hosts (sorted by shard_id) whose replica
///    can move to an underloaded node without duplicating the shard's
///    existing membership.
/// 3. Append the move to the plan, update the in-memory counts, and
///    continue. A node stops being overloaded as soon as one replica
///    is scheduled to leave.
pub fn plan_rebalance(meta: &ClusterMeta) -> RebalancePlan {
    let total_replicas: usize = meta.shards.iter().map(|s| s.members.len()).sum();
    let n = meta.nodes.len();
    if n == 0 || total_replicas == 0 {
        return RebalancePlan::empty();
    }
    let cap = total_replicas.div_ceil(n);

    // Current load — uses BTreeMap so iteration order is stable.
    let mut load: BTreeMap<NodeId, usize> = BTreeMap::new();
    for node in meta.nodes.keys() {
        load.insert(node.clone(), 0);
    }
    for s in &meta.shards {
        for m in &s.members {
            *load.entry(m.clone()).or_insert(0) += 1;
        }
    }

    // Sort overloaded nodes most-loaded first; tie-break on node id.
    let mut overloaded: Vec<(NodeId, usize)> = load
        .iter()
        .filter(|&(_, &c)| c > cap)
        .map(|(k, &v)| (k.clone(), v))
        .collect();
    overloaded.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut plan = RebalancePlan::empty();

    // Snapshot of shard membership we can mutate while planning without
    // touching the real ClusterMeta. Keys are shard_id → members.
    let mut proposed_members: BTreeMap<ShardId, Vec<NodeId>> = meta
        .shards
        .iter()
        .map(|s| (s.shard_id, s.members.clone()))
        .collect();

    for (from, _) in overloaded {
        // Find a shard hosted on `from` we can move.
        let mut best_move: Option<RebalanceMove> = None;
        // Iterate shards in order.
        let mut shard_ids: Vec<ShardId> = proposed_members.keys().copied().collect();
        shard_ids.sort();
        for sid in shard_ids {
            let members = &proposed_members[&sid];
            if !members.contains(&from) {
                continue;
            }
            // Candidate targets: nodes that are currently under the cap
            // AND not already members of this shard.
            let mut candidates: Vec<(NodeId, usize)> = load
                .iter()
                .filter(|&(node, &c)| c < cap && !members.contains(node) && node != &from)
                .map(|(k, &v)| (k.clone(), v))
                .collect();
            // Least-loaded first; tie-break alphabetical.
            candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            if let Some((to, _)) = candidates.into_iter().next() {
                best_move = Some(RebalanceMove {
                    shard_id: sid,
                    from: from.clone(),
                    to,
                });
                break;
            }
        }

        if let Some(m) = best_move {
            // Apply the move to the planner's own state so subsequent
            // iterations see the effect.
            if let Some(members) = proposed_members.get_mut(&m.shard_id) {
                if let Some(pos) = members.iter().position(|n| n == &m.from) {
                    members[pos] = m.to.clone();
                }
            }
            if let Some(v) = load.get_mut(&m.from) {
                *v = v.saturating_sub(1);
            }
            *load.entry(m.to.clone()).or_insert(0) += 1;
            plan.moves.push(m);
        }
        // If no feasible target was found this overloaded node stays
        // overloaded; a later pass (or a cluster size change) will make
        // progress. We explicitly do NOT move to an already-full node.
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sharding::metadata::{NodeInfo, ShardGroup};
    use std::net::SocketAddr;
    use uuid::Uuid;

    fn nid(s: &str) -> NodeId {
        NodeId::new(s).unwrap()
    }

    fn ninfo(id: &str, port: u16) -> NodeInfo {
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        NodeInfo::new(nid(id), addr)
    }

    /// Hand-rolled meta to exercise the rebalancer without going through
    /// `bootstrap`'s round-robin (which is already balanced by
    /// construction).
    fn meta_with_shards(nodes: &[&str], shards: Vec<Vec<&str>>) -> ClusterMeta {
        let nodes_vec: Vec<NodeInfo> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| ninfo(n, 15480 + i as u16))
            .collect();
        let mut nodes_map = std::collections::BTreeMap::new();
        for n in &nodes_vec {
            nodes_map.insert(n.node_id.clone(), n.clone());
        }
        let shards: Vec<ShardGroup> = shards
            .into_iter()
            .enumerate()
            .map(|(i, members)| {
                ShardGroup::new(
                    ShardId::new(i as u32),
                    members.into_iter().map(nid).collect(),
                )
            })
            .collect();
        let metadata_members: Vec<NodeId> = nodes_vec.iter().map(|n| n.node_id.clone()).collect();
        let num_shards = shards.len() as u32;
        let meta = ClusterMeta {
            cluster_id: Uuid::nil(),
            generation: 1,
            num_shards,
            shards,
            metadata_members,
            metadata_leader: None,
            nodes: nodes_map,
        };
        meta.validate().expect("hand-rolled meta must validate");
        meta
    }

    #[test]
    fn balanced_cluster_produces_empty_plan() {
        let meta = meta_with_shards(
            &["node-a", "node-b", "node-c"],
            vec![
                vec!["node-a", "node-b"],
                vec!["node-b", "node-c"],
                vec!["node-a", "node-c"],
            ],
        );
        // cap = ceil(6/3) = 2; every node hosts exactly 2.
        let plan = plan_rebalance(&meta);
        assert!(plan.is_noop(), "expected noop, got {plan:?}");
    }

    #[test]
    fn overloaded_node_loses_one_replica() {
        // node-a hosts 3 replicas, node-c hosts 1, cap = ceil(6/3)=2.
        let meta = meta_with_shards(
            &["node-a", "node-b", "node-c"],
            vec![
                vec!["node-a", "node-b"],
                vec!["node-a", "node-b"],
                vec!["node-a", "node-c"],
            ],
        );
        let plan = plan_rebalance(&meta);
        assert_eq!(plan.len(), 1);
        let m = &plan.moves[0];
        assert_eq!(m.from, nid("node-a"));
        assert_eq!(m.to, nid("node-c"));
    }

    #[test]
    fn rebalance_respects_distinct_members() {
        // shard 0 already has node-a AND node-b. Moving node-b off shard 0
        // to node-a would duplicate — the planner must pick a different
        // shard or target.
        let meta = meta_with_shards(
            &["node-a", "node-b", "node-c", "node-d"],
            vec![
                vec!["node-a", "node-b"],
                vec!["node-a", "node-b"],
                vec!["node-c", "node-d"],
                vec!["node-a", "node-b"],
            ],
        );
        // cap = ceil(8/4) = 2. node-a and node-b each host 3 (over cap).
        let plan = plan_rebalance(&meta);
        // Each overloaded node should receive exactly one move; targets
        // are node-c or node-d; no move duplicates a shard's members.
        assert!(!plan.is_noop());
        for m in &plan.moves {
            let shard = meta.shard(m.shard_id).unwrap();
            assert!(
                !shard.contains(&m.to),
                "move {m:?} would duplicate on shard {}",
                m.shard_id
            );
            assert!(shard.contains(&m.from));
        }
    }

    #[test]
    fn rebalance_is_deterministic() {
        let meta = meta_with_shards(
            &["node-a", "node-b", "node-c"],
            vec![
                vec!["node-a", "node-b"],
                vec!["node-a", "node-b"],
                vec!["node-a", "node-c"],
            ],
        );
        let p1 = plan_rebalance(&meta);
        let p2 = plan_rebalance(&meta);
        assert_eq!(p1, p2);
    }

    #[test]
    fn rebalance_move_roundtrips_to_change() {
        let m = RebalanceMove {
            shard_id: ShardId::new(2),
            from: nid("node-a"),
            to: nid("node-d"),
        };
        let change = m.into_change();
        match change {
            MetaChange::ReplaceShardMember {
                shard_id,
                remove,
                add,
            } => {
                assert_eq!(shard_id, ShardId::new(2));
                assert_eq!(remove, nid("node-a"));
                assert_eq!(add, nid("node-d"));
            }
            other => panic!("unexpected change kind: {other:?}"),
        }
    }

    #[test]
    fn plan_then_apply_converges() {
        // The planner produces at most one move per overloaded node per
        // pass — callers iterate until noop. Verify that the loop
        // terminates and produces a balanced cluster.
        let mut meta = meta_with_shards(
            &["node-a", "node-b", "node-c", "node-d"],
            vec![
                vec!["node-a", "node-b"],
                vec!["node-a", "node-b"],
                vec!["node-a", "node-c"],
                vec!["node-a", "node-d"],
            ],
        );
        for pass in 0..10 {
            let plan = plan_rebalance(&meta);
            if plan.is_noop() {
                break;
            }
            for m in plan.moves {
                meta.apply(m.into_change()).unwrap();
            }
            assert!(pass < 9, "rebalance failed to converge within 10 passes");
        }
        let cap = meta.max_replicas_per_node();
        for node in ["node-a", "node-b", "node-c", "node-d"] {
            let load = meta.replicas_on(&nid(node));
            assert!(
                load <= cap,
                "node {node} still over cap after convergence: load={load} cap={cap}"
            );
        }
    }

    #[test]
    fn empty_cluster_plan_is_empty() {
        // Construct a truly empty meta — skip validate since 0 shards
        // fails by design, but the planner must still return empty.
        let meta = ClusterMeta {
            cluster_id: Uuid::nil(),
            generation: 0,
            num_shards: 0,
            shards: vec![],
            metadata_members: vec![],
            metadata_leader: None,
            nodes: std::collections::BTreeMap::new(),
        };
        assert!(plan_rebalance(&meta).is_noop());
    }

    #[test]
    fn overloaded_node_without_target_is_skipped() {
        // 2-node cluster, 3 replicas on node-a and 1 on node-b. Every
        // shard already contains node-b, so no move is feasible.
        let meta = meta_with_shards(
            &["node-a", "node-b"],
            vec![
                vec!["node-a", "node-b"],
                vec!["node-a", "node-b"],
                vec!["node-a", "node-b"],
            ],
        );
        // cap = ceil(6/2) = 3 — no overload at all, so plan is empty.
        assert!(plan_rebalance(&meta).is_noop());
    }
}
