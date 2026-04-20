//! Raft wire-format value types.
//!
//! Every type in this module is `Serialize + Deserialize` via bincode so
//! the Raft transport can send them on the wire with no additional
//! encoding layer. `Term` and `LogIndex` are deliberately `u64` newtypes
//! so a caller cannot accidentally pass one where the other is expected.

use serde::{Deserialize, Serialize};

use crate::sharding::metadata::{NodeId, ShardId};

/// Monotonic election epoch. Starts at 0; every new election bumps it
/// by one across the cluster.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Term(pub u64);

impl Term {
    /// Successor term.
    #[inline]
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "T{}", self.0)
    }
}

/// Dense 1-indexed log position. Index 0 means "before the log" (used
/// by the initial snapshot / prev-log fields).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct LogIndex(pub u64);

impl LogIndex {
    /// Index zero — "before log".
    pub const ZERO: Self = Self(0);

    /// Next index.
    #[inline]
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// Saturating predecessor (stays at 0).
    #[inline]
    #[must_use]
    pub const fn prev(self) -> Self {
        Self(self.0.saturating_sub(1))
    }
}

impl std::fmt::Display for LogIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "I{}", self.0)
    }
}

/// Top-level Raft message exchanged between replicas. All RPCs are
/// encapsulated here; the transport layer just ships opaque bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaftMessage {
    /// Candidate → peers: request a vote for a new term.
    RequestVote(VoteRequest),
    /// Peer → candidate: reply to RequestVote.
    VoteReply(VoteReply),
    /// Leader → follower: append entries + heartbeat.
    AppendEntries(AppendEntries),
    /// Follower → leader: reply to AppendEntries.
    AppendEntriesReply(AppendEntriesReply),
    /// Leader → lagging follower: install a full snapshot.
    InstallSnapshot(InstallSnapshot),
    /// Follower → leader: reply to InstallSnapshot.
    InstallSnapshotReply(InstallSnapshotReply),
}

impl RaftMessage {
    /// Term the message belongs to — useful for staleness checks.
    #[must_use]
    pub fn term(&self) -> Term {
        match self {
            RaftMessage::RequestVote(m) => m.term,
            RaftMessage::VoteReply(m) => m.term,
            RaftMessage::AppendEntries(m) => m.term,
            RaftMessage::AppendEntriesReply(m) => m.term,
            RaftMessage::InstallSnapshot(m) => m.term,
            RaftMessage::InstallSnapshotReply(m) => m.term,
        }
    }
}

/// Envelope sent by the transport: shard + sender + payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaftEnvelope {
    /// Shard this message belongs to. The receiver multiplexes by this.
    pub shard_id: ShardId,
    /// Sender node id.
    pub from: NodeId,
    /// Message body.
    pub message: RaftMessage,
}

/// §4.1 RequestVote RPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteRequest {
    /// Candidate's current term.
    pub term: Term,
    /// Candidate identity.
    pub candidate: NodeId,
    /// Index of candidate's last log entry.
    pub last_log_index: LogIndex,
    /// Term of candidate's last log entry.
    pub last_log_term: Term,
}

/// §4.1 RequestVote reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteReply {
    /// Current term of the replying peer (for candidate to step down).
    pub term: Term,
    /// True iff the peer granted its vote.
    pub granted: bool,
    /// Which peer replied.
    pub from: NodeId,
}

/// §5 AppendEntries RPC — also serves as heartbeat when `entries`
/// is empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendEntries {
    /// Leader's term.
    pub term: Term,
    /// Leader identity, for caching leader hints on followers.
    pub leader: NodeId,
    /// Index of the log entry immediately preceding the new ones.
    pub prev_log_index: LogIndex,
    /// Term of the entry at `prev_log_index`.
    pub prev_log_term: Term,
    /// Raw entry bytes to append. Empty = heartbeat.
    pub entries: Vec<super::log::LogEntry>,
    /// Leader's known commit index.
    pub leader_commit: LogIndex,
}

/// §5 AppendEntries reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendEntriesReply {
    /// Peer's current term.
    pub term: Term,
    /// True iff the follower accepted and appended.
    pub success: bool,
    /// Peer identity.
    pub from: NodeId,
    /// Index of the last entry the follower has on success; on failure
    /// the index at which its log diverges from the leader. Used by
    /// the leader to rewind `next_index` efficiently.
    pub match_index: LogIndex,
}

/// §7 InstallSnapshot RPC. Single-shot (no chunking) for the initial
/// version; the transport enforces an upper bound on frame size so
/// large snapshots fall back to replay of the WAL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSnapshot {
    /// Leader term.
    pub term: Term,
    /// Leader identity.
    pub leader: NodeId,
    /// Last log entry contained in the snapshot (exclusive upper bound
    /// for the follower's log after install).
    pub last_included_index: LogIndex,
    /// Term of the last included entry.
    pub last_included_term: Term,
    /// Opaque snapshot payload (zstd + tar, produced by
    /// [`crate::replication::snapshot`]).
    pub data: Vec<u8>,
}

/// InstallSnapshot reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSnapshotReply {
    /// Peer term.
    pub term: Term,
    /// Peer identity.
    pub from: NodeId,
    /// True iff the snapshot was installed successfully.
    pub installed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_monotonically_increases() {
        let t = Term(7);
        assert_eq!(t.next(), Term(8));
        assert!(t < t.next());
    }

    #[test]
    fn log_index_prev_saturates_at_zero() {
        assert_eq!(LogIndex::ZERO.prev(), LogIndex::ZERO);
        assert_eq!(LogIndex(1).prev(), LogIndex::ZERO);
    }

    #[test]
    fn log_index_next() {
        assert_eq!(LogIndex(5).next(), LogIndex(6));
    }

    #[test]
    fn raft_message_term_matches_variant() {
        let node = NodeId::new("a").unwrap();
        let m = RaftMessage::RequestVote(VoteRequest {
            term: Term(3),
            candidate: node.clone(),
            last_log_index: LogIndex(10),
            last_log_term: Term(2),
        });
        assert_eq!(m.term(), Term(3));
    }

    #[test]
    fn vote_request_roundtrips_through_bincode() {
        let msg = RaftMessage::RequestVote(VoteRequest {
            term: Term(42),
            candidate: NodeId::new("node-a").unwrap(),
            last_log_index: LogIndex(1024),
            last_log_term: Term(40),
        });
        let bytes = bincode::serialize(&msg).unwrap();
        let back: RaftMessage = bincode::deserialize(&bytes).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn envelope_roundtrips_through_bincode() {
        let env = RaftEnvelope {
            shard_id: ShardId::new(2),
            from: NodeId::new("node-a").unwrap(),
            message: RaftMessage::VoteReply(VoteReply {
                term: Term(5),
                granted: true,
                from: NodeId::new("node-b").unwrap(),
            }),
        };
        let bytes = bincode::serialize(&env).unwrap();
        let back: RaftEnvelope = bincode::deserialize(&bytes).unwrap();
        assert_eq!(env, back);
    }
}
