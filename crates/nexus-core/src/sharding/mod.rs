//! V2 sharding primitives.
//!
//! Nexus V2 supports horizontal scaling via hash-based sharding:
//! a cluster of N nodes is partitioned into `num_shards` shards, each
//! replicated across `replica_factor` nodes via per-shard Raft groups.
//! The metadata describing the cluster layout — shard count, replica
//! assignments, the current generation number — is itself stored in a
//! dedicated Raft group called the **metadata group**.
//!
//! This module owns the *pure* sharding primitives: assignment, the
//! metadata data model, rebalancing, and health snapshots. The Raft
//! transport + apply loop lives in [`crate::sharding::raft`]; the
//! coordinator that decomposes queries into shard-local subplans lives
//! in [`crate::coordinator`].
//!
//! # Shape
//!
//! ```text
//! Cluster
//! ├── metadata group (Raft, replicated across all cluster members)
//! └── shards[0..N]
//!     ├── Raft group with R replicas
//!     ├── record stores / indexes / WAL (standard Nexus storage)
//!     └── shard-local executor
//! ```
//!
//! # Invariants
//!
//! * `node_id` → `shard_id` is a pure deterministic function
//!   ([`assign_shard`]). The same `node_id` always lands on the same
//!   shard across restarts.
//! * Relationships live on the shard owning their **source** node; the
//!   destination shard may hold a remote-anchor record for reverse
//!   expands.
//! * Every metadata change bumps [`ClusterMeta::generation`]. Stale
//!   generations are rejected by shards with `ERR_STALE_GEN`.
//! * A single shard replica is the **only writer** to that shard's
//!   storage layer — the Raft apply loop. No direct writes bypass Raft.

pub mod assignment;
pub mod config;
pub mod controller;
pub mod health;
pub mod metadata;
pub mod raft;
pub mod rebalance;

pub use assignment::{assign_shard, shard_for_node, shard_for_node_u64};
pub use config::{PeerEntry, ShardingConfig, ShardingMode};
pub use controller::{
    AddNodeRequest, ClusterController, ClusterStatus, ControllerError, NodeStatus,
    RemoveNodeRequest, ShardHealthProvider, StaticAllHealthy,
};
pub use health::{ReplicaHealth, ShardHealth};
pub use metadata::{
    ClusterMeta, ClusterMetaError, MetaChange, NodeId, NodeInfo, ShardGroup, ShardId, ShardState,
};
pub use rebalance::{RebalanceMove, RebalancePlan, plan_rebalance};
