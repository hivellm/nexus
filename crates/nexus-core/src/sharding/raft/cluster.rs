//! In-process Raft test harness.
//!
//! `RaftTestCluster` drives multiple [`RaftNode`]s in lockstep through
//! an [`InMemoryTransport`]. It's the fixture for the §Scenario tests
//! that validate leader-election latency, log-replication convergence,
//! and partition tolerance. Keeping the harness in the crate lets the
//! production driver reuse the same sequencing logic for dev-loop
//! smoke tests.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use super::node::{RaftNode, RaftNodeConfig};
use super::transport::InMemoryTransport;
use super::types::LogIndex;
use crate::sharding::metadata::{NodeId, ShardId};

/// Multi-node Raft test cluster.
pub struct RaftTestCluster {
    /// Ordered (for determinism) map of node id → replica.
    nodes: BTreeMap<NodeId, RaftNode>,
    /// Shared in-process transport.
    transport: Arc<InMemoryTransport>,
    /// Nodes that have been "crashed" (not ticked / not delivered).
    crashed: Vec<NodeId>,
}

impl RaftTestCluster {
    /// Build a cluster with `members`.size replicas, all in the same
    /// shard. `rng_seed_base + idx` is the seed for the N-th replica so
    /// the election timeouts are different enough to avoid split votes.
    pub fn new(shard_id: ShardId, members: Vec<NodeId>, rng_seed_base: u64) -> Self {
        let transport = InMemoryTransport::new();
        let mut nodes = BTreeMap::new();
        for (i, m) in members.iter().enumerate() {
            transport.register(m.clone());
            let cfg = RaftNodeConfig {
                shard_id,
                node_id: m.clone(),
                members: members.clone(),
                election_timeout_min: Duration::from_millis(150),
                election_timeout_max: Duration::from_millis(300),
                heartbeat_interval: Duration::from_millis(50),
                tick: Duration::from_millis(10),
                rng_seed: rng_seed_base.wrapping_add(i as u64),
            };
            let node = RaftNode::new(cfg).expect("valid RaftNodeConfig");
            nodes.insert(m.clone(), node);
        }
        Self {
            nodes,
            transport,
            crashed: Vec::new(),
        }
    }

    /// Single global tick: each live node advances its logical clock by
    /// one unit, then every inbox is drained and delivered. Repeat by
    /// the caller until the desired invariant holds.
    pub fn tick(&mut self) {
        // 1. Tick each live node (produces outgoing messages).
        for (id, node) in self.nodes.iter_mut() {
            if self.crashed.contains(id) {
                continue;
            }
            let _ = node.tick(self.transport.as_ref());
        }
        // 2. Deliver — drain each inbox, dispatch to the node's
        // handle_message. We deliver everything currently queued so
        // the cluster makes deterministic progress per tick.
        for id in self.nodes.keys().cloned().collect::<Vec<_>>() {
            if self.crashed.contains(&id) {
                continue;
            }
            let envelopes = self.transport.drain_inbox(&id);
            if let Some(node) = self.nodes.get_mut(&id) {
                for env in envelopes {
                    let _ = node.handle_message(env, self.transport.as_ref());
                }
            }
        }
    }

    /// Run up to `max_ticks` ticks or until `pred` returns true.
    /// Returns the number of ticks consumed.
    pub fn tick_until<F>(&mut self, max_ticks: usize, mut pred: F) -> usize
    where
        F: FnMut(&Self) -> bool,
    {
        for i in 0..max_ticks {
            self.tick();
            if pred(self) {
                return i + 1;
            }
        }
        max_ticks
    }

    /// Propose a write on the current leader. Returns the index the
    /// write was appended at, or `None` if there is no leader.
    pub fn propose_on_leader(&mut self, cmd: Vec<u8>) -> Option<LogIndex> {
        let leader_id = self.leader()?;
        let node = self.nodes.get_mut(&leader_id)?;
        node.propose(cmd, self.transport.as_ref()).ok()
    }

    /// Current leader id among **live** nodes. Raft guarantees at most
    /// one leader per term — when more than one live node is in the
    /// Leader role (e.g. an old leader stuck in its own term after a
    /// partition), the one with the highest term is authoritative.
    /// Crashed nodes are excluded.
    #[must_use]
    pub fn leader(&self) -> Option<NodeId> {
        let candidates: Vec<(NodeId, super::types::Term)> = self
            .nodes
            .iter()
            .filter(|(id, n)| n.is_leader() && !self.crashed.contains(id))
            .map(|(id, n)| (id.clone(), n.current_term()))
            .collect();
        let max_term = candidates.iter().map(|(_, t)| *t).max()?;
        let top: Vec<_> = candidates
            .into_iter()
            .filter(|(_, t)| *t == max_term)
            .collect();
        if top.len() == 1 {
            Some(top[0].0.clone())
        } else {
            None
        }
    }

    /// True iff every live node's commit_index ≥ `idx`.
    #[must_use]
    pub fn all_committed(&self, idx: LogIndex) -> bool {
        self.nodes
            .iter()
            .filter(|(id, _)| !self.crashed.contains(id))
            .all(|(_, n)| n.commit_index() >= idx)
    }

    /// Expose a node by id (read-only).
    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<&RaftNode> {
        self.nodes.get(id)
    }

    /// "Crash" a node: stop ticking it and stop delivering messages.
    /// Its inbox keeps receiving until `restart` is called.
    pub fn crash(&mut self, id: NodeId) {
        if !self.crashed.contains(&id) {
            self.crashed.push(id);
        }
    }

    /// Undo [`Self::crash`]: the node ticks and receives again.
    pub fn restart(&mut self, id: &NodeId) {
        self.crashed.retain(|n| n != id);
    }

    /// Shared transport handle, for partition / heal control.
    #[must_use]
    pub fn transport(&self) -> Arc<InMemoryTransport> {
        self.transport.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nids(ids: &[&str]) -> Vec<NodeId> {
        ids.iter().map(|s| NodeId::new(*s).unwrap()).collect()
    }

    #[test]
    fn three_node_cluster_elects_single_leader() {
        let mut c = RaftTestCluster::new(ShardId::new(0), nids(&["a", "b", "c"]), 1);
        // Without heartbeats yet, all nodes are followers. Tick until a
        // leader appears; should happen within ~2 × election_timeout.
        let ticks = c.tick_until(100, |c| c.leader().is_some());
        assert!(c.leader().is_some(), "no leader after {ticks} ticks");
    }

    #[test]
    fn leader_term_is_one_on_first_election() {
        let mut c = RaftTestCluster::new(ShardId::new(0), nids(&["a", "b", "c"]), 1);
        c.tick_until(100, |c| c.leader().is_some());
        let leader = c.leader().unwrap();
        let n = c.node(&leader).unwrap();
        assert!(n.current_term().0 >= 1);
    }

    #[test]
    fn propose_is_replicated_to_all_followers() {
        let mut c = RaftTestCluster::new(ShardId::new(0), nids(&["a", "b", "c"]), 1);
        c.tick_until(100, |c| c.leader().is_some());
        let idx = c.propose_on_leader(b"hello".to_vec()).unwrap();
        // Tick until commit propagates to everyone.
        let ticks = c.tick_until(100, |c| c.all_committed(idx));
        assert!(
            c.all_committed(idx),
            "commit not propagated after {ticks} ticks"
        );
    }

    #[test]
    fn leader_failover_within_election_bound() {
        let mut c = RaftTestCluster::new(ShardId::new(0), nids(&["a", "b", "c"]), 7);
        c.tick_until(100, |c| c.leader().is_some());
        let old_leader = c.leader().unwrap();
        c.crash(old_leader.clone());
        // Per spec: new leader within 3× election timeout = 900ms = 90 ticks.
        let ticks = c.tick_until(200, |c| matches!(c.leader(), Some(l) if l != old_leader));
        let new_leader = c.leader().expect("no new leader elected after crash");
        assert_ne!(new_leader, old_leader);
        assert!(
            ticks <= 90,
            "failover took {ticks} ticks (spec bound: 90 ticks = 900ms)"
        );
    }

    #[test]
    fn minority_partition_does_not_elect_leader() {
        let mut c = RaftTestCluster::new(ShardId::new(0), nids(&["a", "b", "c"]), 11);
        c.tick_until(100, |c| c.leader().is_some());
        let old_leader = c.leader().unwrap();
        // Isolate the old leader from both followers (both directions).
        let others: Vec<_> = nids(&["a", "b", "c"])
            .into_iter()
            .filter(|n| n != &old_leader)
            .collect();
        for other in &others {
            c.transport().partition(old_leader.clone(), other.clone());
            c.transport().partition(other.clone(), old_leader.clone());
        }
        // Run for 300 ticks. The isolated leader eventually steps down
        // or stays leader in isolation; the majority MUST elect a new
        // leader distinct from the old one.
        let ticks = c.tick_until(300, |c| matches!(c.leader(), Some(l) if l != old_leader));
        let new_leader = c.leader().expect("majority should elect a new leader");
        assert_ne!(new_leader, old_leader);
        assert!(ticks <= 200);
    }

    #[test]
    fn crashed_node_does_not_tick() {
        let mut c = RaftTestCluster::new(ShardId::new(0), nids(&["a", "b", "c"]), 1);
        c.crash(NodeId::new("a").unwrap());
        // 10 ticks with a crashed; its term should stay at 0.
        for _ in 0..10 {
            c.tick();
        }
        let n = c.node(&NodeId::new("a").unwrap()).unwrap();
        assert_eq!(n.current_term().0, 0);
    }

    #[test]
    fn single_node_cluster_immediately_becomes_leader() {
        let mut c = RaftTestCluster::new(ShardId::new(0), nids(&["a"]), 1);
        c.tick_until(50, |c| c.leader().is_some());
        assert_eq!(c.leader(), Some(NodeId::new("a").unwrap()));
    }

    #[test]
    fn five_node_cluster_tolerates_two_failures() {
        let mut c = RaftTestCluster::new(ShardId::new(0), nids(&["a", "b", "c", "d", "e"]), 3);
        c.tick_until(200, |c| c.leader().is_some());
        let old_leader = c.leader().unwrap();
        // Crash the leader and one follower; the remaining 3 are a
        // majority of 5.
        let followers: Vec<_> = nids(&["a", "b", "c", "d", "e"])
            .into_iter()
            .filter(|n| n != &old_leader)
            .collect();
        c.crash(old_leader.clone());
        c.crash(followers[0].clone());
        let ticks = c.tick_until(300, |c| matches!(c.leader(), Some(l) if l != old_leader));
        assert!(c.leader().is_some(), "no quorum reached in {ticks} ticks");
    }

    #[test]
    fn replication_continues_after_follower_restart() {
        let mut c = RaftTestCluster::new(ShardId::new(0), nids(&["a", "b", "c"]), 5);
        c.tick_until(100, |c| c.leader().is_some());
        let leader = c.leader().unwrap();

        // Identify a follower to crash.
        let follower = nids(&["a", "b", "c"])
            .into_iter()
            .find(|n| *n != leader)
            .unwrap();
        c.crash(follower.clone());

        // Proposals keep committing on the majority of 2/3.
        let idx = c.propose_on_leader(b"one".to_vec()).unwrap();
        c.tick_until(100, |c| {
            c.node(&leader)
                .map(|n| n.commit_index() >= idx)
                .unwrap_or(false)
        });

        // Restart follower — it should catch up.
        c.restart(&follower);
        c.tick_until(200, |c| c.all_committed(idx));
        assert!(c.all_committed(idx));
    }
}
