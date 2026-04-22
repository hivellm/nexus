//! Query classification.
//!
//! The coordinator needs to decide for every query: **single-shard**,
//! **targeted** (a subset), or **broadcast**. This module does the
//! classification given explicit hints from the upper layer — the
//! Cypher parser / planner is responsible for recognizing patterns
//! like `MATCH (n:Label {id: $x})` and producing `ClassifyHints`.
//! Keeping the parser out of this crate means the coordinator is
//! usable from any layer that can build a `ClassifyHints`.

use serde::{Deserialize, Serialize};

use crate::sharding::assignment::shard_for_node_u64;
use crate::sharding::metadata::ShardId;

/// Which shards a query targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryScope {
    /// Exactly one shard. The most efficient case — no merge needed
    /// beyond forwarding.
    SingleShard(ShardId),
    /// A specific subset of shards.
    Targeted(Vec<ShardId>),
    /// Every shard in the cluster. The scatter/gather engine expands
    /// this at runtime using the current [`crate::sharding::metadata::ClusterMeta`].
    Broadcast,
}

impl Default for QueryScope {
    fn default() -> Self {
        Self::Broadcast
    }
}

/// Hints the parser / planner passes to [`classify`]. Each field is
/// optional; absence means the parser could not extract that hint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassifyHints {
    /// Explicit list of target shards — overrides everything else.
    /// Set by operators / tests that want to pin a query.
    pub pinned_shards: Option<Vec<ShardId>>,
    /// The query contains an equality predicate on the sharding key
    /// of exactly one root pattern, with a literal / parameter value.
    /// `Some(storage_node_id)` lets the classifier compute the shard.
    pub sharding_key_value: Option<u64>,
    /// The query is a pure-write (`CREATE` / `MERGE` / `DELETE`) that
    /// the parser determined touches only the shard owning its
    /// source node, given `sharding_key_value` above.
    pub write_local_to_source: bool,
}

/// A classified query: scope + classification metadata useful for
/// observability and response headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassifiedQuery {
    /// Target shards for this query.
    pub scope: QueryScope,
    /// Short human label for the classification reason; surfaced in
    /// tracing spans.
    pub reason: String,
}

/// Classify a query given parser hints and the current shard count.
/// `num_shards` must be > 0.
///
/// # Panics
///
/// Panics when `num_shards == 0`, which violates a cluster invariant
/// (every cluster has at least one shard). The panic is a programmer
/// error, not a runtime condition.
#[must_use]
pub fn classify(hints: &ClassifyHints, num_shards: u32) -> ClassifiedQuery {
    assert!(num_shards > 0, "classify called with num_shards == 0");

    if let Some(ref pinned) = hints.pinned_shards {
        if pinned.len() == 1 {
            return ClassifiedQuery {
                scope: QueryScope::SingleShard(pinned[0]),
                reason: "pinned:single".into(),
            };
        }
        return ClassifiedQuery {
            scope: QueryScope::Targeted(pinned.clone()),
            reason: "pinned:targeted".into(),
        };
    }

    if let Some(id) = hints.sharding_key_value {
        let shard = shard_for_node_u64(&id, num_shards);
        return ClassifiedQuery {
            scope: QueryScope::SingleShard(shard),
            reason: if hints.write_local_to_source {
                "shard-key+source-local-write"
            } else {
                "shard-key"
            }
            .into(),
        };
    }

    ClassifiedQuery {
        scope: QueryScope::Broadcast,
        reason: "no-hints".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_hints_is_broadcast() {
        let c = classify(&ClassifyHints::default(), 4);
        assert_eq!(c.scope, QueryScope::Broadcast);
        assert_eq!(c.reason, "no-hints");
    }

    #[test]
    fn sharding_key_pins_to_one_shard() {
        let c = classify(
            &ClassifyHints {
                sharding_key_value: Some(42),
                ..Default::default()
            },
            4,
        );
        match c.scope {
            QueryScope::SingleShard(s) => {
                // Must match the assignment function's choice.
                assert_eq!(s, shard_for_node_u64(&42, 4));
            }
            other => panic!("expected SingleShard, got {other:?}"),
        }
        assert_eq!(c.reason, "shard-key");
    }

    #[test]
    fn source_local_write_reason_tagged() {
        let c = classify(
            &ClassifyHints {
                sharding_key_value: Some(42),
                write_local_to_source: true,
                ..Default::default()
            },
            4,
        );
        assert_eq!(c.reason, "shard-key+source-local-write");
    }

    #[test]
    fn pinned_single_overrides_shard_key() {
        let c = classify(
            &ClassifyHints {
                pinned_shards: Some(vec![ShardId::new(3)]),
                sharding_key_value: Some(42),
                ..Default::default()
            },
            4,
        );
        assert_eq!(c.scope, QueryScope::SingleShard(ShardId::new(3)));
        assert_eq!(c.reason, "pinned:single");
    }

    #[test]
    fn pinned_multi_is_targeted() {
        let c = classify(
            &ClassifyHints {
                pinned_shards: Some(vec![ShardId::new(0), ShardId::new(2)]),
                ..Default::default()
            },
            4,
        );
        match c.scope {
            QueryScope::Targeted(s) => {
                assert_eq!(s, vec![ShardId::new(0), ShardId::new(2)]);
            }
            other => panic!("expected Targeted, got {other:?}"),
        }
        assert_eq!(c.reason, "pinned:targeted");
    }

    #[test]
    #[should_panic(expected = "num_shards == 0")]
    fn zero_shards_panics() {
        let _ = classify(&ClassifyHints::default(), 0);
    }

    #[test]
    fn single_shard_cluster_always_single() {
        let c = classify(&ClassifyHints::default(), 1);
        // With 1 shard, broadcast == single shard in effect, but the
        // classification still reports Broadcast. The scatter engine
        // short-circuits the broadcast to a single RPC.
        assert_eq!(c.scope, QueryScope::Broadcast);
    }
}
