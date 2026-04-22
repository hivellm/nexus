//! [`RaftNode`] — the per-replica finite-state machine.
//!
//! Pure, synchronous, clock-driven. No I/O, no threads. The caller:
//!
//! 1. Advances the logical clock with [`RaftNode::tick`].
//! 2. Delivers incoming messages with [`RaftNode::handle_message`].
//! 3. Accepts client writes with [`RaftNode::propose`].
//! 4. Drains committed entries with [`RaftNode::drain_committed`].
//!
//! This factoring is deliberate: the harness tests in [`super::cluster`]
//! run many `RaftNode`s in a single thread and step them in lockstep,
//! which makes election-latency and log-replication tests deterministic.
//! A production driver wraps each [`RaftNode`] in a tokio task whose
//! loop calls the four methods above.

use std::collections::BTreeSet;
use std::time::Duration;

use super::log::{LogEntry, RaftLog};
use super::state::{LeaderState, PersistentState, RaftRole};
use super::transport::{RaftTransport, TransportError};
use super::types::{
    AppendEntries, AppendEntriesReply, InstallSnapshot, InstallSnapshotReply, LogIndex,
    RaftEnvelope, RaftMessage, Term, VoteReply, VoteRequest,
};
use crate::sharding::metadata::{NodeId, ShardId};

/// Batch size (entries per AppendEntries) used for follower catch-up.
const MAX_ENTRIES_PER_APPEND: usize = 64;

/// Tunable knobs for a Raft replica.
#[derive(Debug, Clone)]
pub struct RaftNodeConfig {
    /// Shard this replica serves.
    pub shard_id: ShardId,
    /// Stable id of this replica.
    pub node_id: NodeId,
    /// Full Raft-group membership (includes `node_id`).
    pub members: Vec<NodeId>,
    /// Minimum election timeout. §5.2.
    pub election_timeout_min: Duration,
    /// Maximum election timeout. §5.2.
    pub election_timeout_max: Duration,
    /// Leader heartbeat period.
    pub heartbeat_interval: Duration,
    /// Fixed logical-clock tick size. Required so the FSM is
    /// deterministic. Must evenly divide every timeout value that's
    /// compared against it.
    pub tick: Duration,
    /// Deterministic RNG seed for randomizing the election timeout.
    /// In production, set this to the low 64 bits of the node's UUID
    /// or a boot entropy byte; tests pin it explicitly.
    pub rng_seed: u64,
}

impl RaftNodeConfig {
    /// Validate timing invariants. Rejects configurations that would
    /// make Raft progress impossible (heartbeat ≥ min election timeout,
    /// etc.).
    pub fn validate(&self) -> Result<(), String> {
        if self.election_timeout_min >= self.election_timeout_max {
            return Err("election_timeout_min must be < election_timeout_max".into());
        }
        if self.heartbeat_interval >= self.election_timeout_min {
            return Err("heartbeat_interval must be < election_timeout_min".into());
        }
        if self.tick.is_zero() {
            return Err("tick must be > 0".into());
        }
        if !self.members.iter().any(|m| m == &self.node_id) {
            return Err("node_id must appear in members".into());
        }
        let mut seen = BTreeSet::new();
        for m in &self.members {
            if !seen.insert(m.clone()) {
                return Err(format!("duplicate member {m}"));
            }
        }
        Ok(())
    }

    /// Peers = members minus self.
    #[must_use]
    pub fn peers(&self) -> Vec<NodeId> {
        self.members
            .iter()
            .filter(|m| **m != self.node_id)
            .cloned()
            .collect()
    }
}

/// A single Raft replica's FSM.
pub struct RaftNode {
    cfg: RaftNodeConfig,
    persistent: PersistentState,
    log: RaftLog,
    role: RaftRole,
    commit_index: LogIndex,
    last_applied: LogIndex,
    /// Elapsed time since the last valid leader heartbeat (or campaign
    /// start, for candidates). Reset on election-timer events.
    election_elapsed: Duration,
    /// Elapsed time since the last heartbeat fan-out (leaders only).
    heartbeat_elapsed: Duration,
    /// Randomized election timeout for the current term. Re-rolled on
    /// each role change into follower or candidate.
    current_election_timeout: Duration,
    /// Counter for the deterministic PRNG used to randomize the
    /// election timeout. Seeded from `cfg.rng_seed`.
    rng_state: u64,
    /// Entries the caller has drained from via `drain_committed`.
    /// Separate from `commit_index` because the caller applies
    /// asynchronously.
    drained_through: LogIndex,
}

impl RaftNode {
    /// Construct a replica with default persistent state (fresh boot).
    pub fn new(cfg: RaftNodeConfig) -> Result<Self, String> {
        Self::with_state(cfg, PersistentState::default(), RaftLog::new())
    }

    /// Construct a replica from recovered state (after restart).
    pub fn with_state(
        cfg: RaftNodeConfig,
        persistent: PersistentState,
        log: RaftLog,
    ) -> Result<Self, String> {
        cfg.validate()?;
        let commit_index = log.snapshot_meta().0;
        let mut node = Self {
            cfg,
            persistent,
            log,
            role: RaftRole::initial(),
            commit_index,
            last_applied: commit_index,
            drained_through: commit_index,
            election_elapsed: Duration::ZERO,
            heartbeat_elapsed: Duration::ZERO,
            current_election_timeout: Duration::ZERO,
            rng_state: 0,
        };
        node.rng_state = node.cfg.rng_seed;
        node.reset_election_timer();
        Ok(node)
    }

    // ------------------------------------------------------------------
    // Read-only accessors (used by tests + the outer driver)
    // ------------------------------------------------------------------

    /// Current role.
    #[inline]
    #[must_use]
    pub fn role(&self) -> &RaftRole {
        &self.role
    }

    /// True iff this replica is the current leader.
    #[inline]
    #[must_use]
    pub fn is_leader(&self) -> bool {
        self.role.is_leader()
    }

    /// Current term.
    #[inline]
    #[must_use]
    pub fn current_term(&self) -> Term {
        self.persistent.current_term
    }

    /// Last committed index.
    #[inline]
    #[must_use]
    pub fn commit_index(&self) -> LogIndex {
        self.commit_index
    }

    /// Last index written to the local log.
    #[inline]
    #[must_use]
    pub fn last_log_index(&self) -> LogIndex {
        self.log.last_index()
    }

    /// Current leader hint, if one is known.
    #[must_use]
    pub fn leader_hint(&self) -> Option<&NodeId> {
        match &self.role {
            RaftRole::Follower { current_leader } => current_leader.as_ref(),
            RaftRole::Leader(_) => Some(&self.cfg.node_id),
            RaftRole::Candidate { .. } => None,
        }
    }

    /// Immutable log reference — tests use this to assert contents.
    #[inline]
    #[must_use]
    pub fn log(&self) -> &RaftLog {
        &self.log
    }

    // ------------------------------------------------------------------
    // Driver hooks
    // ------------------------------------------------------------------

    /// Advance the logical clock by one `tick`. Triggers election
    /// timeouts on followers/candidates and heartbeats on leaders.
    pub fn tick(&mut self, transport: &dyn RaftTransport) -> Result<(), TransportError> {
        self.election_elapsed += self.cfg.tick;
        self.heartbeat_elapsed += self.cfg.tick;
        match &self.role {
            RaftRole::Follower { .. } | RaftRole::Candidate { .. } => {
                if self.election_elapsed >= self.current_election_timeout {
                    self.start_election(transport)?;
                }
            }
            RaftRole::Leader(_) => {
                if self.heartbeat_elapsed >= self.cfg.heartbeat_interval {
                    self.broadcast_heartbeats(transport)?;
                    self.heartbeat_elapsed = Duration::ZERO;
                }
            }
        }
        Ok(())
    }

    /// Propose a client write. Only leaders accept; on a follower
    /// returns the cached leader hint via `Err`. Returns the index the
    /// write was appended at on success.
    pub fn propose(
        &mut self,
        command: Vec<u8>,
        transport: &dyn RaftTransport,
    ) -> Result<LogIndex, ProposeError> {
        if !self.is_leader() {
            return Err(ProposeError::NotLeader {
                leader_hint: self.leader_hint().cloned(),
            });
        }
        let term = self.persistent.current_term;
        let idx = self.log.append_command(term, command);
        // Update our own match.
        if let RaftRole::Leader(ls) = &mut self.role {
            ls.match_index.insert(self.cfg.node_id.clone(), idx);
        }
        // Fan out immediately. Errors on individual peers are non-fatal.
        let _ = self.broadcast_append(transport);
        self.maybe_advance_commit();
        Ok(idx)
    }

    /// Handle an envelope from `transport`. Returns `Ok` regardless of
    /// whether the message was valid — invalid / stale messages are
    /// dropped silently per §5.1 ("current terms are exchanged when
    /// servers communicate; if one server's current term is smaller
    /// than the other's, then it updates its current term...").
    pub fn handle_message(
        &mut self,
        env: RaftEnvelope,
        transport: &dyn RaftTransport,
    ) -> Result<(), TransportError> {
        // §5.1 term update — any incoming higher term forces us back to
        // Follower before we process the message.
        if env.message.term() > self.persistent.current_term {
            self.become_follower(env.message.term(), None);
        }
        match env.message {
            RaftMessage::RequestVote(req) => self.on_request_vote(req, transport)?,
            RaftMessage::VoteReply(rep) => self.on_vote_reply(rep, transport)?,
            RaftMessage::AppendEntries(ae) => self.on_append_entries(ae, transport)?,
            RaftMessage::AppendEntriesReply(rep) => self.on_append_reply(rep, transport)?,
            RaftMessage::InstallSnapshot(snap) => self.on_install_snapshot(snap, transport)?,
            RaftMessage::InstallSnapshotReply(rep) => self.on_install_snapshot_reply(rep),
        }
        Ok(())
    }

    /// Drain committed but not-yet-applied entries. Caller is expected
    /// to apply them to the state machine and persist `last_applied`.
    /// Returns entries in ascending index order.
    #[must_use]
    pub fn drain_committed(&mut self) -> Vec<LogEntry> {
        let mut out = Vec::new();
        let from = self.drained_through.next();
        if from > self.commit_index {
            return out;
        }
        let mut idx = from;
        while idx <= self.commit_index {
            if let Some(entry) = self.log.entry_at(idx).cloned() {
                out.push(entry);
                idx = idx.next();
            } else {
                break;
            }
        }
        self.drained_through = self.commit_index;
        self.last_applied = self.commit_index;
        out
    }

    // ------------------------------------------------------------------
    // Role transitions
    // ------------------------------------------------------------------

    fn become_follower(&mut self, term: Term, leader: Option<NodeId>) {
        if term > self.persistent.current_term {
            self.persistent.current_term = term;
            self.persistent.voted_for = None;
        }
        self.role = RaftRole::Follower {
            current_leader: leader,
        };
        self.reset_election_timer();
    }

    fn become_candidate(&mut self) {
        self.persistent.current_term = self.persistent.current_term.next();
        self.persistent.voted_for = Some(self.cfg.node_id.clone());
        self.role = RaftRole::Candidate {
            votes_received: vec![self.cfg.node_id.clone()],
        };
        self.reset_election_timer();
    }

    fn become_leader(&mut self, transport: &dyn RaftTransport) -> Result<(), TransportError> {
        let peers = self.cfg.peers();
        let leader_state = LeaderState::new(&peers, self.log.last_index());
        self.role = RaftRole::Leader(leader_state);
        // Our own match_index includes everything in our log.
        if let RaftRole::Leader(ls) = &mut self.role {
            ls.match_index
                .insert(self.cfg.node_id.clone(), self.log.last_index());
        }
        self.heartbeat_elapsed = Duration::ZERO;
        // Immediate no-op heartbeat to establish authority quickly.
        self.broadcast_heartbeats(transport)
    }

    // ------------------------------------------------------------------
    // Election
    // ------------------------------------------------------------------

    fn start_election(&mut self, transport: &dyn RaftTransport) -> Result<(), TransportError> {
        self.become_candidate();
        let req = VoteRequest {
            term: self.persistent.current_term,
            candidate: self.cfg.node_id.clone(),
            last_log_index: self.log.last_index(),
            last_log_term: self.log.last_term(),
        };
        // Single-node clusters win instantly (self-vote is a majority
        // of 1).
        if self.cfg.members.len() == 1 {
            self.become_leader(transport)?;
            return Ok(());
        }
        for p in self.cfg.peers() {
            let env = RaftEnvelope {
                shard_id: self.cfg.shard_id,
                from: self.cfg.node_id.clone(),
                message: RaftMessage::RequestVote(req.clone()),
            };
            let _ = transport.send(&p, env);
        }
        Ok(())
    }

    fn on_request_vote(
        &mut self,
        req: VoteRequest,
        transport: &dyn RaftTransport,
    ) -> Result<(), TransportError> {
        let grant = {
            // §5.2: reject if candidate's term is stale.
            if req.term < self.persistent.current_term {
                false
            } else if self.persistent.voted_for.is_some()
                && self.persistent.voted_for.as_ref() != Some(&req.candidate)
            {
                false
            } else {
                // §5.4.1: "up-to-date" log check.
                let my_last_term = self.log.last_term();
                let my_last_index = self.log.last_index();
                let log_ok = req.last_log_term > my_last_term
                    || (req.last_log_term == my_last_term && req.last_log_index >= my_last_index);
                if log_ok {
                    self.persistent.voted_for = Some(req.candidate.clone());
                    self.reset_election_timer();
                    true
                } else {
                    false
                }
            }
        };
        let reply = VoteReply {
            term: self.persistent.current_term,
            granted: grant,
            from: self.cfg.node_id.clone(),
        };
        let env = RaftEnvelope {
            shard_id: self.cfg.shard_id,
            from: self.cfg.node_id.clone(),
            message: RaftMessage::VoteReply(reply),
        };
        transport.send(&req.candidate, env)
    }

    fn on_vote_reply(
        &mut self,
        rep: VoteReply,
        transport: &dyn RaftTransport,
    ) -> Result<(), TransportError> {
        // Only candidates care.
        if !matches!(self.role, RaftRole::Candidate { .. }) {
            return Ok(());
        }
        if rep.term != self.persistent.current_term {
            return Ok(());
        }
        if !rep.granted {
            return Ok(());
        }
        let become_leader = if let RaftRole::Candidate { votes_received } = &mut self.role {
            if !votes_received.contains(&rep.from) {
                votes_received.push(rep.from);
            }
            let majority = self.cfg.members.len() / 2 + 1;
            votes_received.len() >= majority
        } else {
            false
        };
        if become_leader {
            self.become_leader(transport)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Log replication
    // ------------------------------------------------------------------

    fn broadcast_heartbeats(
        &mut self,
        transport: &dyn RaftTransport,
    ) -> Result<(), TransportError> {
        self.broadcast_append(transport)
    }

    fn broadcast_append(&mut self, transport: &dyn RaftTransport) -> Result<(), TransportError> {
        if !self.is_leader() {
            return Ok(());
        }
        for peer in self.cfg.peers() {
            self.send_append_to(&peer, transport)?;
        }
        Ok(())
    }

    fn send_append_to(
        &mut self,
        peer: &NodeId,
        transport: &dyn RaftTransport,
    ) -> Result<(), TransportError> {
        let (prev_idx, prev_term, entries) = {
            let ls = match &self.role {
                RaftRole::Leader(ls) => ls,
                _ => return Ok(()),
            };
            let next = *ls.next_index.get(peer).unwrap_or(&LogIndex(1));
            let prev_idx = next.prev();
            let prev_term = self.log.term_at(prev_idx).unwrap_or(Term(0));
            let entries = self.log.entries_from(next, MAX_ENTRIES_PER_APPEND).to_vec();
            (prev_idx, prev_term, entries)
        };
        let ae = AppendEntries {
            term: self.persistent.current_term,
            leader: self.cfg.node_id.clone(),
            prev_log_index: prev_idx,
            prev_log_term: prev_term,
            entries,
            leader_commit: self.commit_index,
        };
        let env = RaftEnvelope {
            shard_id: self.cfg.shard_id,
            from: self.cfg.node_id.clone(),
            message: RaftMessage::AppendEntries(ae),
        };
        transport.send(peer, env)
    }

    fn on_append_entries(
        &mut self,
        ae: AppendEntries,
        transport: &dyn RaftTransport,
    ) -> Result<(), TransportError> {
        // Caller already bumped our term if ae.term > ours.
        let (success, match_index) = if ae.term < self.persistent.current_term {
            (false, LogIndex::ZERO)
        } else {
            // Refresh leader hint + election timer.
            self.role = RaftRole::Follower {
                current_leader: Some(ae.leader.clone()),
            };
            self.reset_election_timer();

            // §5.3: reject if prev_log_index doesn't exist or its term
            // disagrees.
            let prev_matches = if ae.prev_log_index == LogIndex::ZERO {
                true
            } else {
                self.log.term_at(ae.prev_log_index) == Some(ae.prev_log_term)
            };
            if !prev_matches {
                // Hint: where we actually stop agreeing.
                let hint = self.log.last_index().min(ae.prev_log_index);
                (false, hint)
            } else {
                let last = self.log.append_follower(ae.entries);
                if ae.leader_commit > self.commit_index {
                    self.commit_index = ae.leader_commit.min(last);
                }
                (true, last)
            }
        };
        let reply = AppendEntriesReply {
            term: self.persistent.current_term,
            success,
            from: self.cfg.node_id.clone(),
            match_index,
        };
        let env = RaftEnvelope {
            shard_id: self.cfg.shard_id,
            from: self.cfg.node_id.clone(),
            message: RaftMessage::AppendEntriesReply(reply),
        };
        transport.send(&ae.leader, env)
    }

    fn on_append_reply(
        &mut self,
        rep: AppendEntriesReply,
        transport: &dyn RaftTransport,
    ) -> Result<(), TransportError> {
        if !self.is_leader() {
            return Ok(());
        }
        if rep.term != self.persistent.current_term {
            return Ok(());
        }
        let retry_peer = {
            if let RaftRole::Leader(ls) = &mut self.role {
                if rep.success {
                    ls.on_append_success(&rep.from, rep.match_index);
                } else {
                    ls.on_append_reject(&rep.from, rep.match_index);
                }
            }
            !rep.success
        };
        self.maybe_advance_commit();
        if retry_peer {
            self.send_append_to(&rep.from, transport)?;
        }
        Ok(())
    }

    fn maybe_advance_commit(&mut self) {
        let new_commit = if let RaftRole::Leader(ls) = &self.role {
            let term = self.persistent.current_term;
            let log = &self.log;
            ls.compute_commit(
                log.last_index(),
                term,
                |idx| log.term_at(idx),
                self.cfg.members.len(),
            )
        } else {
            return;
        };
        if new_commit > self.commit_index {
            self.commit_index = new_commit;
        }
    }

    // ------------------------------------------------------------------
    // Snapshot install
    // ------------------------------------------------------------------

    fn on_install_snapshot(
        &mut self,
        snap: InstallSnapshot,
        transport: &dyn RaftTransport,
    ) -> Result<(), TransportError> {
        if snap.term < self.persistent.current_term {
            return Ok(());
        }
        self.role = RaftRole::Follower {
            current_leader: Some(snap.leader.clone()),
        };
        self.reset_election_timer();
        self.log
            .install_snapshot(snap.last_included_index, snap.last_included_term);
        if snap.last_included_index > self.commit_index {
            self.commit_index = snap.last_included_index;
            self.drained_through = snap.last_included_index;
            self.last_applied = snap.last_included_index;
        }
        let reply = InstallSnapshotReply {
            term: self.persistent.current_term,
            from: self.cfg.node_id.clone(),
            installed: true,
        };
        let env = RaftEnvelope {
            shard_id: self.cfg.shard_id,
            from: self.cfg.node_id.clone(),
            message: RaftMessage::InstallSnapshotReply(reply),
        };
        transport.send(&snap.leader, env)
    }

    fn on_install_snapshot_reply(&mut self, rep: InstallSnapshotReply) {
        if !self.is_leader() || !rep.installed {
            return;
        }
        if let RaftRole::Leader(ls) = &mut self.role {
            let snap_idx = self.log.snapshot_meta().0;
            ls.on_append_success(&rep.from, snap_idx);
        }
    }

    // ------------------------------------------------------------------
    // Timers
    // ------------------------------------------------------------------

    fn reset_election_timer(&mut self) {
        self.election_elapsed = Duration::ZERO;
        self.current_election_timeout = self.random_election_timeout();
    }

    /// Deterministic xorshift RNG — gives reproducible election timings
    /// in tests. Range is
    /// `[election_timeout_min, election_timeout_max)` rounded to the
    /// tick.
    fn random_election_timeout(&mut self) -> Duration {
        // xorshift64*
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        let min = self.cfg.election_timeout_min.as_millis() as u64;
        let max = self.cfg.election_timeout_max.as_millis() as u64;
        let span = max.saturating_sub(min);
        let pick = if span == 0 { 0 } else { x % span };
        Duration::from_millis(min + pick)
    }
}

/// Reasons a `propose` call rejects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposeError {
    /// Not the leader — the client should retry against `leader_hint`
    /// if any.
    NotLeader { leader_hint: Option<NodeId> },
}

impl std::fmt::Display for ProposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotLeader {
                leader_hint: Some(h),
            } => {
                write!(f, "not leader (hint: {h})")
            }
            Self::NotLeader { leader_hint: None } => write!(f, "not leader (no hint)"),
        }
    }
}
impl std::error::Error for ProposeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sharding::raft::transport::InMemoryTransport;

    fn nid(s: &str) -> NodeId {
        NodeId::new(s).unwrap()
    }

    fn cfg(node: &str, members: &[&str], seed: u64) -> RaftNodeConfig {
        RaftNodeConfig {
            shard_id: ShardId::new(0),
            node_id: nid(node),
            members: members.iter().map(|m| nid(m)).collect(),
            election_timeout_min: Duration::from_millis(150),
            election_timeout_max: Duration::from_millis(300),
            heartbeat_interval: Duration::from_millis(50),
            tick: Duration::from_millis(10),
            rng_seed: seed,
        }
    }

    #[test]
    fn config_validate_rejects_missing_self() {
        let mut c = cfg("a", &["b", "c"], 1);
        assert!(c.validate().is_err());
        c.members.push(nid("a"));
        assert!(c.validate().is_ok());
    }

    #[test]
    fn config_validate_rejects_duplicates() {
        let c = cfg("a", &["a", "a"], 1);
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_validate_rejects_bad_timers() {
        let mut c = cfg("a", &["a"], 1);
        c.heartbeat_interval = c.election_timeout_min;
        assert!(c.validate().is_err());
    }

    #[test]
    fn new_node_starts_as_follower() {
        let node = RaftNode::new(cfg("a", &["a", "b", "c"], 1)).unwrap();
        assert_eq!(node.role().name(), "follower");
        assert_eq!(node.current_term(), Term(0));
    }

    #[test]
    fn single_node_cluster_elects_itself_on_timeout() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        let mut n = RaftNode::new(cfg("a", &["a"], 1)).unwrap();
        // Tick until election fires.
        for _ in 0..50 {
            n.tick(t.as_ref()).unwrap();
            if n.is_leader() {
                break;
            }
        }
        assert!(n.is_leader());
        assert_eq!(n.current_term(), Term(1));
    }

    #[test]
    fn follower_grants_vote_once_per_term() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        t.register(nid("b"));
        t.register(nid("c"));
        let mut n = RaftNode::new(cfg("a", &["a", "b", "c"], 1)).unwrap();

        let req = VoteRequest {
            term: Term(1),
            candidate: nid("b"),
            last_log_index: LogIndex::ZERO,
            last_log_term: Term(0),
        };
        n.handle_message(
            RaftEnvelope {
                shard_id: ShardId::new(0),
                from: nid("b"),
                message: RaftMessage::RequestVote(req.clone()),
            },
            t.as_ref(),
        )
        .unwrap();
        let mut inbox = t.drain_inbox(&nid("b"));
        assert_eq!(inbox.len(), 1);
        let reply = inbox.pop().unwrap();
        if let RaftMessage::VoteReply(r) = reply.message {
            assert!(r.granted);
        } else {
            panic!();
        }

        // Second candidate in the same term must be rejected.
        let req2 = VoteRequest {
            term: Term(1),
            candidate: nid("c"),
            last_log_index: LogIndex::ZERO,
            last_log_term: Term(0),
        };
        n.handle_message(
            RaftEnvelope {
                shard_id: ShardId::new(0),
                from: nid("c"),
                message: RaftMessage::RequestVote(req2),
            },
            t.as_ref(),
        )
        .unwrap();
        let inbox2 = t.drain_inbox(&nid("c"));
        assert_eq!(inbox2.len(), 1);
        if let RaftMessage::VoteReply(r) = &inbox2[0].message {
            assert!(!r.granted, "should not grant two votes in same term");
        }
    }

    #[test]
    fn propose_on_follower_returns_hint() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        let mut n = RaftNode::new(cfg("a", &["a", "b", "c"], 1)).unwrap();
        let err = n.propose(b"cmd".to_vec(), t.as_ref()).unwrap_err();
        match err {
            ProposeError::NotLeader { leader_hint } => {
                assert!(leader_hint.is_none());
            }
        }
    }

    #[test]
    fn higher_term_message_reverts_to_follower() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        t.register(nid("b"));
        let mut n = RaftNode::new(cfg("a", &["a"], 1)).unwrap();
        // Force self to leader by running a single-node election.
        for _ in 0..50 {
            n.tick(t.as_ref()).unwrap();
            if n.is_leader() {
                break;
            }
        }
        assert!(n.is_leader());
        t.drain_inbox(&nid("a"));

        // Deliver a higher-term heartbeat — must revert to follower.
        let ae = AppendEntries {
            term: Term(99),
            leader: nid("b"),
            prev_log_index: LogIndex::ZERO,
            prev_log_term: Term(0),
            entries: vec![],
            leader_commit: LogIndex::ZERO,
        };
        n.handle_message(
            RaftEnvelope {
                shard_id: ShardId::new(0),
                from: nid("b"),
                message: RaftMessage::AppendEntries(ae),
            },
            t.as_ref(),
        )
        .unwrap();
        assert!(!n.is_leader());
        assert_eq!(n.current_term(), Term(99));
    }

    #[test]
    fn follower_rejects_append_with_mismatched_prev_term() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        t.register(nid("b"));
        let mut n = RaftNode::new(cfg("a", &["a", "b"], 1)).unwrap();
        // Pre-seed a local log entry at (term=1, idx=1).
        let _ = n.log.append_command(Term(1), b"x".to_vec());

        let ae = AppendEntries {
            term: Term(2),
            leader: nid("b"),
            prev_log_index: LogIndex(1),
            prev_log_term: Term(3), // WRONG — our term=1 at index=1.
            entries: vec![],
            leader_commit: LogIndex::ZERO,
        };
        n.handle_message(
            RaftEnvelope {
                shard_id: ShardId::new(0),
                from: nid("b"),
                message: RaftMessage::AppendEntries(ae),
            },
            t.as_ref(),
        )
        .unwrap();
        let inbox = t.drain_inbox(&nid("b"));
        assert_eq!(inbox.len(), 1);
        if let RaftMessage::AppendEntriesReply(rep) = &inbox[0].message {
            assert!(!rep.success);
        } else {
            panic!();
        }
    }

    #[test]
    fn drain_committed_returns_in_order() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        let mut n = RaftNode::new(cfg("a", &["a"], 1)).unwrap();
        for _ in 0..50 {
            n.tick(t.as_ref()).unwrap();
            if n.is_leader() {
                break;
            }
        }
        assert!(n.is_leader());
        n.propose(b"one".to_vec(), t.as_ref()).unwrap();
        n.propose(b"two".to_vec(), t.as_ref()).unwrap();
        n.propose(b"three".to_vec(), t.as_ref()).unwrap();
        let committed = n.drain_committed();
        let cmds: Vec<_> = committed.iter().map(|e| e.command.clone()).collect();
        assert_eq!(
            cmds,
            vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]
        );
        // Second drain returns nothing new.
        assert!(n.drain_committed().is_empty());
    }

    #[test]
    fn leader_hint_tracks_last_seen_leader() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        t.register(nid("b"));
        let mut n = RaftNode::new(cfg("a", &["a", "b"], 1)).unwrap();
        let ae = AppendEntries {
            term: Term(1),
            leader: nid("b"),
            prev_log_index: LogIndex::ZERO,
            prev_log_term: Term(0),
            entries: vec![],
            leader_commit: LogIndex::ZERO,
        };
        n.handle_message(
            RaftEnvelope {
                shard_id: ShardId::new(0),
                from: nid("b"),
                message: RaftMessage::AppendEntries(ae),
            },
            t.as_ref(),
        )
        .unwrap();
        assert_eq!(n.leader_hint(), Some(&nid("b")));
    }

    #[test]
    fn install_snapshot_advances_commit() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        t.register(nid("b"));
        let mut n = RaftNode::new(cfg("a", &["a", "b"], 1)).unwrap();
        let snap = InstallSnapshot {
            term: Term(5),
            leader: nid("b"),
            last_included_index: LogIndex(100),
            last_included_term: Term(4),
            data: vec![0u8; 64],
        };
        n.handle_message(
            RaftEnvelope {
                shard_id: ShardId::new(0),
                from: nid("b"),
                message: RaftMessage::InstallSnapshot(snap),
            },
            t.as_ref(),
        )
        .unwrap();
        assert_eq!(n.commit_index(), LogIndex(100));
        assert_eq!(n.current_term(), Term(5));
    }

    #[test]
    fn propose_on_leader_appends_and_returns_index() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        let mut n = RaftNode::new(cfg("a", &["a"], 1)).unwrap();
        for _ in 0..50 {
            n.tick(t.as_ref()).unwrap();
            if n.is_leader() {
                break;
            }
        }
        let idx = n.propose(b"hello".to_vec(), t.as_ref()).unwrap();
        assert_eq!(idx, LogIndex(1));
        assert_eq!(n.last_log_index(), LogIndex(1));
    }
}
