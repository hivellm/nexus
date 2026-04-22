//! Raft role state machine.
//!
//! Split out of [`super::node`] so the role transitions + leader
//! bookkeeping are individually unit-testable.

use std::collections::BTreeMap;

use super::types::{LogIndex, Term};
use crate::sharding::metadata::NodeId;

/// Role of a Raft replica at a given moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaftRole {
    /// Not yet started / no leader election attempted.
    Follower {
        /// Cached leader for this term, if one sent us a heartbeat.
        current_leader: Option<NodeId>,
    },
    /// Campaigning for the current term.
    Candidate {
        /// Set of peers that have voted for us (including ourselves).
        votes_received: Vec<NodeId>,
    },
    /// Accepted as leader by a majority in the current term.
    Leader(LeaderState),
}

impl RaftRole {
    /// Fresh-boot role: Follower, no leader.
    #[must_use]
    pub fn initial() -> Self {
        Self::Follower {
            current_leader: None,
        }
    }

    /// True when this replica should accept client writes.
    #[inline]
    #[must_use]
    pub fn is_leader(&self) -> bool {
        matches!(self, Self::Leader(_))
    }

    /// Short human-readable role name, used in logs + tests.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Follower { .. } => "follower",
            Self::Candidate { .. } => "candidate",
            Self::Leader(_) => "leader",
        }
    }
}

/// Per-peer bookkeeping maintained only by the leader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderState {
    /// For each follower, the index of the next entry to send. §5.3.
    pub next_index: BTreeMap<NodeId, LogIndex>,
    /// For each follower, the highest log entry known to be replicated.
    pub match_index: BTreeMap<NodeId, LogIndex>,
}

impl LeaderState {
    /// Initialize leader bookkeeping from the leader's own log state.
    /// `peers` is the current shard membership MINUS self; `last_index`
    /// is the leader's last log index (entries ≤ `last_index` are
    /// candidates to be replicated).
    #[must_use]
    pub fn new(peers: &[NodeId], last_index: LogIndex) -> Self {
        let mut next_index = BTreeMap::new();
        let mut match_index = BTreeMap::new();
        for p in peers {
            next_index.insert(p.clone(), last_index.next());
            match_index.insert(p.clone(), LogIndex::ZERO);
        }
        Self {
            next_index,
            match_index,
        }
    }

    /// Handle a successful AppendEntries reply from `peer` covering up
    /// to and including `match_index`.
    pub fn on_append_success(&mut self, peer: &NodeId, new_match: LogIndex) {
        // §5.3 — next_index must always be > match_index.
        let mi = self
            .match_index
            .entry(peer.clone())
            .or_insert(LogIndex::ZERO);
        if new_match > *mi {
            *mi = new_match;
        }
        let ni = self.next_index.entry(peer.clone()).or_insert(LogIndex(1));
        let target = new_match.next();
        if target > *ni {
            *ni = target;
        }
    }

    /// Handle a rejected AppendEntries — rewind `next_index` by one so
    /// the leader retries with an earlier prefix. §5.3.
    pub fn on_append_reject(&mut self, peer: &NodeId, hint: LogIndex) {
        let ni = self.next_index.entry(peer.clone()).or_insert(LogIndex(1));
        // Prefer the follower's hint when it's strictly earlier than
        // our current next_index; otherwise step back by one.
        let stepped = LogIndex(ni.0.saturating_sub(1).max(1));
        let target = if hint.0 > 0 && hint < *ni {
            hint
        } else {
            stepped
        };
        if target < *ni {
            *ni = target;
        }
    }

    /// Compute the commit index given the leader's log size + its
    /// own `current_term`: the highest index `N` such that a majority
    /// of `match_index` values are ≥ `N` **and** the entry at `N` is
    /// from `current_term`. §5.4.2 — leaders only commit entries from
    /// their own term.
    #[must_use]
    pub fn compute_commit(
        &self,
        leader_match: LogIndex,
        leader_term: Term,
        term_at: impl Fn(LogIndex) -> Option<Term>,
        cluster_size: usize,
    ) -> LogIndex {
        // Collect all match indices including leader's own.
        let mut indices: Vec<LogIndex> = self.match_index.values().copied().collect();
        indices.push(leader_match);
        indices.sort();
        // Majority = cluster_size / 2 + 1. The N-th percentile from the
        // top that achieves majority is at position `cluster_size -
        // majority`.
        if cluster_size == 0 {
            return LogIndex::ZERO;
        }
        let majority = cluster_size / 2 + 1;
        if indices.len() < majority {
            return LogIndex::ZERO;
        }
        // `indices` has `cluster_size` elements; the index that a
        // majority has reached is the one at position `len - majority`
        // after sorting ascending (think: top `majority` values).
        let idx = indices[indices.len() - majority];
        if idx == LogIndex::ZERO {
            return LogIndex::ZERO;
        }
        // Only commit entries from the leader's current term.
        if term_at(idx) == Some(leader_term) {
            idx
        } else {
            LogIndex::ZERO
        }
    }
}

/// Persistent Raft state (the parts §5.1 requires to survive crashes).
/// In this implementation the `RaftNode` holds these in memory and the
/// outer shard's storage layer is responsible for flushing them;
/// splitting them out keeps the persistence contract explicit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersistentState {
    /// Latest term the server has seen.
    pub current_term: Term,
    /// Candidate this replica voted for in `current_term`, if any.
    pub voted_for: Option<NodeId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(s: &str) -> NodeId {
        NodeId::new(s).unwrap()
    }

    #[test]
    fn initial_role_is_follower() {
        let role = RaftRole::initial();
        assert!(!role.is_leader());
        assert_eq!(role.name(), "follower");
    }

    #[test]
    fn leader_state_initializes_peers_to_last_plus_one() {
        let peers = vec![nid("b"), nid("c")];
        let ls = LeaderState::new(&peers, LogIndex(7));
        assert_eq!(ls.next_index[&nid("b")], LogIndex(8));
        assert_eq!(ls.next_index[&nid("c")], LogIndex(8));
        assert_eq!(ls.match_index[&nid("b")], LogIndex::ZERO);
    }

    #[test]
    fn append_success_advances_match_and_next() {
        let peers = vec![nid("b")];
        let mut ls = LeaderState::new(&peers, LogIndex(3));
        ls.on_append_success(&nid("b"), LogIndex(5));
        assert_eq!(ls.match_index[&nid("b")], LogIndex(5));
        assert_eq!(ls.next_index[&nid("b")], LogIndex(6));
    }

    #[test]
    fn append_success_does_not_regress_match() {
        let peers = vec![nid("b")];
        let mut ls = LeaderState::new(&peers, LogIndex(3));
        ls.on_append_success(&nid("b"), LogIndex(5));
        ls.on_append_success(&nid("b"), LogIndex(4));
        assert_eq!(ls.match_index[&nid("b")], LogIndex(5));
    }

    #[test]
    fn append_reject_steps_back() {
        let peers = vec![nid("b")];
        let mut ls = LeaderState::new(&peers, LogIndex(5));
        ls.on_append_reject(&nid("b"), LogIndex::ZERO);
        assert_eq!(ls.next_index[&nid("b")], LogIndex(5));
    }

    #[test]
    fn append_reject_uses_follower_hint_when_lower() {
        let peers = vec![nid("b")];
        let mut ls = LeaderState::new(&peers, LogIndex(10));
        ls.on_append_reject(&nid("b"), LogIndex(3));
        assert_eq!(ls.next_index[&nid("b")], LogIndex(3));
    }

    #[test]
    fn compute_commit_three_node_majority() {
        // Cluster: leader with match=5, followers b=5, c=3.
        // Majority of 3 = 2. Top-2 values are [5, 5]. min = 5.
        let peers = vec![nid("b"), nid("c")];
        let mut ls = LeaderState::new(&peers, LogIndex(0));
        ls.on_append_success(&nid("b"), LogIndex(5));
        ls.on_append_success(&nid("c"), LogIndex(3));
        let commit = ls.compute_commit(LogIndex(5), Term(1), |_| Some(Term(1)), 3);
        assert_eq!(commit, LogIndex(5));
    }

    #[test]
    fn compute_commit_refuses_stale_term_entries() {
        let peers = vec![nid("b"), nid("c")];
        let mut ls = LeaderState::new(&peers, LogIndex(0));
        ls.on_append_success(&nid("b"), LogIndex(5));
        ls.on_append_success(&nid("c"), LogIndex(5));
        // Entry at 5 is from a previous term — §5.4.2 forbids commit.
        let commit = ls.compute_commit(LogIndex(5), Term(2), |_| Some(Term(1)), 3);
        assert_eq!(commit, LogIndex::ZERO);
    }

    #[test]
    fn compute_commit_needs_majority() {
        let peers = vec![nid("b"), nid("c"), nid("d"), nid("e")];
        let mut ls = LeaderState::new(&peers, LogIndex(0));
        ls.on_append_success(&nid("b"), LogIndex(5));
        ls.on_append_success(&nid("c"), LogIndex(5));
        // Only 3/5 nodes have 5 (leader + b + c). Majority = 3 — exactly met.
        let commit = ls.compute_commit(LogIndex(5), Term(1), |_| Some(Term(1)), 5);
        assert_eq!(commit, LogIndex(5));
    }

    #[test]
    fn compute_commit_no_progress_without_majority() {
        let peers = vec![nid("b"), nid("c"), nid("d"), nid("e")];
        let mut ls = LeaderState::new(&peers, LogIndex(0));
        ls.on_append_success(&nid("b"), LogIndex(5));
        // Only 2/5 nodes (leader + b) have reached 5. Majority = 3.
        let commit = ls.compute_commit(LogIndex(5), Term(1), |_| Some(Term(1)), 5);
        assert_eq!(commit, LogIndex::ZERO);
    }

    #[test]
    fn compute_commit_single_node_cluster() {
        let ls = LeaderState::new(&[], LogIndex(0));
        let commit = ls.compute_commit(LogIndex(7), Term(1), |_| Some(Term(1)), 1);
        assert_eq!(commit, LogIndex(7));
    }

    #[test]
    fn persistent_state_default_is_term_zero() {
        let p = PersistentState::default();
        assert_eq!(p.current_term, Term(0));
        assert!(p.voted_for.is_none());
    }
}
