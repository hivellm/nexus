//! Distributed query plan representation.
//!
//! `DistributedPlan` is the shape the coordinator receives from the
//! upper layer (the `nexus-server` REST handler or a future SDK). It
//! describes the query's logical intent in shard-aware terms; the
//! coordinator decomposes it via [`DistributedPlan::decompose`] into a
//! [`DecomposedPlan`] that the scatter/gather engine executes.
//!
//! Keeping the plan type abstract over the Cypher executor avoids
//! reaching into the executor's internal AST — every integration point
//! (Cypher parser, GraphQL bridge, MCP tools) can build a
//! `DistributedPlan` without shared types. The coordinator is tested
//! against hand-constructed plans; integration with the parser is
//! wired in the server crate.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::classify::QueryScope;
use super::merge::MergeOp;

/// One row returned from a shard. Mirrors the REST surface's row
/// format (`[[value1, value2, ...]]`).
pub type Row = Vec<Value>;

/// A query plan the coordinator executes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DistributedPlan {
    /// Cypher text (or analogous plan serialization) the shard-local
    /// executor runs. Opaque to the coordinator.
    pub shard_local_cypher: String,
    /// Parameter bindings passed through unchanged.
    #[serde(default)]
    pub parameters: serde_json::Map<String, Value>,
    /// Ordered list of column names the plan returns. The coordinator
    /// does not interpret them; they're forwarded in the final
    /// response.
    pub columns: Vec<String>,
    /// Classification target — which shards to hit.
    pub scope: QueryScope,
    /// Merge operator applied over per-shard results at the
    /// coordinator.
    #[serde(default)]
    pub merge: MergeOp,
}

impl DistributedPlan {
    /// Decompose into a [`DecomposedPlan`] ready for scatter/gather.
    /// This is deliberately pure — the hard scheduling work is in
    /// [`super::scatter`].
    #[must_use]
    pub fn decompose(self) -> DecomposedPlan {
        DecomposedPlan {
            shard_local_cypher: self.shard_local_cypher,
            parameters: self.parameters,
            columns: self.columns,
            scope: self.scope,
            merge: self.merge,
        }
    }
}

/// A plan after decomposition. Structurally identical to
/// [`DistributedPlan`] — the decomposition step is currently a
/// projection, kept as its own type because that's the natural
/// extension point where rewrite passes (pushdown, re-sort, limit
/// merge) will live in V2.1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecomposedPlan {
    /// Cypher to send to each targeted shard.
    pub shard_local_cypher: String,
    /// Parameters forwarded to shards.
    pub parameters: serde_json::Map<String, Value>,
    /// Result column names.
    pub columns: Vec<String>,
    /// Target shards.
    pub scope: QueryScope,
    /// Merge operator applied over per-shard results.
    pub merge: MergeOp,
}

impl DecomposedPlan {
    /// Number of target shards (0 for broadcast to all, delegated to
    /// the scatter engine).
    #[must_use]
    pub fn target_shard_count(&self, num_shards: u32) -> u32 {
        match &self.scope {
            QueryScope::SingleShard(_) => 1,
            QueryScope::Targeted(s) => s.len() as u32,
            QueryScope::Broadcast => num_shards,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sharding::metadata::ShardId;

    #[test]
    fn decompose_is_identity_projection() {
        let plan = DistributedPlan {
            shard_local_cypher: "MATCH (n) RETURN n".into(),
            parameters: Default::default(),
            columns: vec!["n".into()],
            scope: QueryScope::SingleShard(ShardId::new(2)),
            merge: MergeOp::Concat,
        };
        let dec = plan.decompose();
        assert_eq!(dec.shard_local_cypher, "MATCH (n) RETURN n");
        assert_eq!(dec.scope, QueryScope::SingleShard(ShardId::new(2)));
    }

    #[test]
    fn target_shard_count_matches_scope() {
        let dec = DecomposedPlan {
            shard_local_cypher: String::new(),
            parameters: Default::default(),
            columns: vec![],
            scope: QueryScope::Targeted(vec![ShardId::new(0), ShardId::new(3)]),
            merge: MergeOp::Concat,
        };
        assert_eq!(dec.target_shard_count(8), 2);

        let bcast = DecomposedPlan {
            scope: QueryScope::Broadcast,
            ..dec.clone()
        };
        assert_eq!(bcast.target_shard_count(8), 8);

        let single = DecomposedPlan {
            scope: QueryScope::SingleShard(ShardId::new(0)),
            ..dec.clone()
        };
        assert_eq!(single.target_shard_count(8), 1);
    }

    #[test]
    fn plan_roundtrips_through_json() {
        let plan = DistributedPlan {
            shard_local_cypher: "MATCH (n:Person {id: $x}) RETURN n".into(),
            parameters: {
                let mut m = serde_json::Map::new();
                m.insert("x".into(), Value::from(42));
                m
            },
            columns: vec!["n".into()],
            scope: QueryScope::SingleShard(ShardId::new(0)),
            merge: MergeOp::Concat,
        };
        let s = serde_json::to_string(&plan).unwrap();
        let back: DistributedPlan = serde_json::from_str(&s).unwrap();
        assert_eq!(plan, back);
    }
}
