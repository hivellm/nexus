//! Raft transport abstraction.
//!
//! The [`RaftTransport`] trait is the narrow interface [`super::node::RaftNode`]
//! uses to send [`RaftMessage`]s. Keeping it async-free and non-generic
//! over the runtime means the core Raft FSM stays synchronous, which in
//! turn makes the state-transition tests deterministic.
//!
//! Two implementations live in the crate:
//!
//! * [`InMemoryTransport`] — `#[cfg(test)]` + production-usable. Queues
//!   messages in per-recipient `VecDeque`s. Used by [`super::cluster`]
//!   to drive multi-node Raft tests in a single thread.
//! * A real TCP transport is provided by `crate::sharding::network`
//!   (Phase 2 wire layer). The TCP transport lives outside this module
//!   because it pulls in tokio + `tokio::net`, which this module
//!   deliberately avoids.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use thiserror::Error;

use super::types::RaftEnvelope;
use crate::sharding::metadata::{NodeId, ShardId};

/// Errors a transport may surface to [`super::node::RaftNode`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransportError {
    /// No route known for the target node. Non-fatal — Raft will retry.
    #[error("no route to node {0}")]
    UnknownNode(NodeId),
    /// Underlying I/O failed. Non-fatal; Raft retries next heartbeat.
    #[error("transport I/O error: {0}")]
    Io(String),
}

/// Bidirectional Raft transport.
pub trait RaftTransport: Send + Sync {
    /// Enqueue `env` for delivery to `target`. Non-blocking; the
    /// transport must not call back into [`super::node::RaftNode`]
    /// during this call.
    fn send(&self, target: &NodeId, env: RaftEnvelope) -> Result<(), TransportError>;
}

/// In-process test transport. Messages are delivered synchronously via
/// `drain_inbox`; no background task, no sockets.
///
/// Shared between many [`super::node::RaftNode`]s by wrapping it in an
/// `Arc`. All state lives behind a single mutex — the workload for the
/// harness is low, contention is fine.
#[derive(Debug, Default)]
pub struct InMemoryTransport {
    inner: Mutex<InMemoryTransportInner>,
}

#[derive(Debug, Default)]
struct InMemoryTransportInner {
    /// Per-recipient inbox.
    inboxes: BTreeMap<NodeId, VecDeque<RaftEnvelope>>,
    /// Pairs of (from, to) that have been explicitly partitioned —
    /// `send` to those pairs is silently dropped. Used by jepsen-style
    /// tests to simulate split-brain scenarios.
    partitions: Vec<(NodeId, NodeId)>,
}

impl InMemoryTransport {
    /// Fresh transport with no known nodes and no partitions.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Register `node` so `send`s to it land in its inbox.
    pub fn register(&self, node: NodeId) {
        let mut inner = self.inner.lock().expect("InMemoryTransport mutex poisoned");
        inner.inboxes.entry(node).or_default();
    }

    /// Partition traffic from `from` to `to` (one-directional). Both
    /// directions must be called to simulate a full network split.
    pub fn partition(&self, from: NodeId, to: NodeId) {
        let mut inner = self.inner.lock().expect("InMemoryTransport mutex poisoned");
        let pair = (from, to);
        if !inner.partitions.contains(&pair) {
            inner.partitions.push(pair);
        }
    }

    /// Lift an existing partition.
    pub fn heal(&self, from: &NodeId, to: &NodeId) {
        let mut inner = self.inner.lock().expect("InMemoryTransport mutex poisoned");
        inner.partitions.retain(|(a, b)| !(a == from && b == to));
    }

    /// Drain `node`'s inbox. Returns the messages in delivery order.
    #[must_use]
    pub fn drain_inbox(&self, node: &NodeId) -> Vec<RaftEnvelope> {
        let mut inner = self.inner.lock().expect("InMemoryTransport mutex poisoned");
        inner
            .inboxes
            .get_mut(node)
            .map(|q| q.drain(..).collect())
            .unwrap_or_default()
    }

    /// True iff `node` has any queued messages.
    #[must_use]
    pub fn inbox_len(&self, node: &NodeId) -> usize {
        let inner = self.inner.lock().expect("InMemoryTransport mutex poisoned");
        inner.inboxes.get(node).map(VecDeque::len).unwrap_or(0)
    }

    /// Total in-flight messages across every inbox. Used by
    /// [`super::cluster::RaftTestCluster`] to detect quiescence.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        let inner = self.inner.lock().expect("InMemoryTransport mutex poisoned");
        inner.inboxes.values().map(VecDeque::len).sum()
    }
}

impl RaftTransport for InMemoryTransport {
    fn send(&self, target: &NodeId, env: RaftEnvelope) -> Result<(), TransportError> {
        let mut inner = self.inner.lock().expect("InMemoryTransport mutex poisoned");
        // Drop partitioned traffic silently.
        if inner
            .partitions
            .iter()
            .any(|(a, b)| a == &env.from && b == target)
        {
            return Ok(());
        }
        match inner.inboxes.get_mut(target) {
            Some(q) => {
                q.push_back(env);
                Ok(())
            }
            None => Err(TransportError::UnknownNode(target.clone())),
        }
    }
}

/// Marker helper used by the wire layer to compute the 4-byte shard
/// prefix prepended to every frame. Wire format:
/// `[shard_id:u32 LE][payload]`.
#[must_use]
pub fn encode_shard_prefix(shard_id: ShardId) -> [u8; 4] {
    shard_id.as_u32().to_le_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sharding::raft::types::{LogIndex, RaftMessage, Term, VoteRequest};

    fn nid(s: &str) -> NodeId {
        NodeId::new(s).unwrap()
    }

    fn sample_envelope(from: &str, shard: u32) -> RaftEnvelope {
        RaftEnvelope {
            shard_id: ShardId::new(shard),
            from: nid(from),
            message: RaftMessage::RequestVote(VoteRequest {
                term: Term(1),
                candidate: nid(from),
                last_log_index: LogIndex::ZERO,
                last_log_term: Term(0),
            }),
        }
    }

    #[test]
    fn send_to_unregistered_returns_error() {
        let t = InMemoryTransport::new();
        let err = t.send(&nid("ghost"), sample_envelope("a", 0)).unwrap_err();
        assert!(matches!(err, TransportError::UnknownNode(_)));
    }

    #[test]
    fn registered_node_receives_envelope() {
        let t = InMemoryTransport::new();
        t.register(nid("b"));
        t.send(&nid("b"), sample_envelope("a", 0)).unwrap();
        let drained = t.drain_inbox(&nid("b"));
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].from, nid("a"));
    }

    #[test]
    fn drain_is_fifo() {
        let t = InMemoryTransport::new();
        t.register(nid("b"));
        t.send(&nid("b"), sample_envelope("a", 0)).unwrap();
        t.send(&nid("b"), sample_envelope("c", 0)).unwrap();
        let drained = t.drain_inbox(&nid("b"));
        assert_eq!(drained[0].from, nid("a"));
        assert_eq!(drained[1].from, nid("c"));
    }

    #[test]
    fn drain_clears_inbox() {
        let t = InMemoryTransport::new();
        t.register(nid("b"));
        t.send(&nid("b"), sample_envelope("a", 0)).unwrap();
        t.drain_inbox(&nid("b"));
        assert_eq!(t.inbox_len(&nid("b")), 0);
    }

    #[test]
    fn partition_drops_from_to_traffic() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        t.register(nid("b"));
        t.partition(nid("a"), nid("b"));
        t.send(&nid("b"), sample_envelope("a", 0)).unwrap();
        assert_eq!(t.inbox_len(&nid("b")), 0);
    }

    #[test]
    fn heal_restores_traffic() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        t.register(nid("b"));
        t.partition(nid("a"), nid("b"));
        t.heal(&nid("a"), &nid("b"));
        t.send(&nid("b"), sample_envelope("a", 0)).unwrap();
        assert_eq!(t.inbox_len(&nid("b")), 1);
    }

    #[test]
    fn partition_is_directional() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        t.register(nid("b"));
        t.partition(nid("a"), nid("b"));
        // B → A still flows because partition was A → B only.
        t.send(&nid("a"), sample_envelope("b", 0)).unwrap();
        assert_eq!(t.inbox_len(&nid("a")), 1);
    }

    #[test]
    fn pending_count_aggregates_inboxes() {
        let t = InMemoryTransport::new();
        t.register(nid("a"));
        t.register(nid("b"));
        t.send(&nid("a"), sample_envelope("c", 0)).unwrap();
        t.send(&nid("b"), sample_envelope("c", 0)).unwrap();
        t.send(&nid("a"), sample_envelope("d", 0)).unwrap();
        assert_eq!(t.pending_count(), 3);
    }

    #[test]
    fn shard_prefix_is_little_endian() {
        let bytes = encode_shard_prefix(ShardId::new(0x01020304));
        assert_eq!(bytes, [0x04, 0x03, 0x02, 0x01]);
    }
}
