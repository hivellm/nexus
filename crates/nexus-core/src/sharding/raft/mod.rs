//! Per-shard Raft consensus.
//!
//! This is a purpose-built Raft implementation tuned for Nexus shards.
//! Rather than pulling in `openraft` — whose current release is
//! `0.10.0-alpha.17` (not yet stable) and whose trait surface (3 traits,
//! 60+ methods, heavy use of GATs) would require an adapter larger than
//! the Raft itself — we implement the subset of Raft the specs demand:
//!
//! * Leader election within `3 × election_timeout` after a leader stops
//!   sending heartbeats.
//! * Majority-quorum log replication: a write commits once `R/2 + 1`
//!   replicas have appended it.
//! * Snapshot install for bootstrapping a new replica (reuses the
//!   existing `replication::Snapshot` zstd+tar format).
//! * Wire format matching the project convention:
//!   `[shard_id:u32][message_type:u8][length:u32][payload:N][crc32:u32]`.
//!
//! # Structure
//!
//! * [`types`] — small value types shared across the implementation.
//! * [`log`] — append-only Raft log + an abstraction over persistence.
//! * [`state`] — the Raft role state machine (Follower/Candidate/Leader).
//! * [`node`] — [`RaftNode`], the per-replica FSM driver. Pure: pushes
//!   messages out through a [`RaftTransport`], reacts to incoming
//!   messages + clock ticks. No I/O, no async, no thread spawning.
//! * [`transport`] — the [`RaftTransport`] trait + an
//!   [`InMemoryTransport`] for deterministic multi-node tests.
//! * [`cluster`] — [`RaftTestCluster`], a `#[cfg(test)]`-only harness
//!   that drives multiple [`RaftNode`]s in lockstep through an
//!   [`InMemoryTransport`]. Used by the §Scenario tests for
//!   leader-election latency and log replication.
//!
//! # Single-writer invariant
//!
//! Nexus storage is single-writer per partition. Inside a shard this
//! stays true: only the Raft leader's apply loop touches the storage
//! layer. [`RaftNode`] exposes committed entries via
//! [`RaftNode::drain_committed`], which the shard's apply loop pulls
//! from serially.

pub mod cluster;
pub mod codec;
pub mod log;
pub mod node;
pub mod state;
pub mod tcp_transport;
pub mod transport;
pub mod types;

pub use codec::{CodecError, FrameHeader, FrameIoError, decode_frame, encode_frame};
pub use log::{LogEntry, RaftLog};
pub use node::{RaftNode, RaftNodeConfig};
pub use state::{LeaderState, RaftRole};
pub use tcp_transport::{TcpRaftTransport, TcpRaftTransportConfig};
pub use transport::{InMemoryTransport, RaftTransport, TransportError};
pub use types::{
    AppendEntries, AppendEntriesReply, LogIndex, RaftMessage, Term, VoteReply, VoteRequest,
};
