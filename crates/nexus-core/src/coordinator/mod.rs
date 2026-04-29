//! Distributed query coordinator.
//!
//! The coordinator sits above the per-shard executors and provides a
//! single-query surface to clients in a sharded deployment. Its job:
//!
//! 1. **Classify** a query as either single-shard, targeted (a subset
//!    of shards), or broadcast. See [`classify`].
//! 2. **Decompose** a logical plan into shard-local subplans + a
//!    coordinator-level merge operator. See [`plan`].
//! 3. **Scatter/gather** the subplans to shard leaders in parallel,
//!    merging the per-shard results. See [`scatter`].
//! 4. **Enforce atomicity**: any shard failure fails the entire query;
//!    partial rows are never returned to clients. See
//!    [`scatter::CoordinatorError`].
//!
//! The coordinator is generic over a [`scatter::ShardClient`] trait so
//! unit tests drive it against an in-memory implementation
//! ([`scatter::InMemoryShardClient`]) without needing a real RPC
//! stack. The production wiring plugs in the TCP transport at the
//! `nexus-server` layer.
//!
//! # Row model
//!
//! Shards return rows in the same Neo4j-compatible array format the
//! REST surface exposes: `Vec<Vec<serde_json::Value>>`. The coordinator
//! operates on `Row = Vec<Value>` to stay compatible with the rest of
//! Nexus end-to-end without importing the executor's internal row
//! types.

pub mod classify;
pub mod cross_shard;
pub mod merge;
pub mod multi_shard_tx;
pub mod plan;
pub mod scatter;
pub mod tcp_client;

pub use classify::{ClassifiedQuery, ClassifyHints, QueryScope};
pub use cross_shard::{
    CrossShardCache, CrossShardError, FetchBudget, InMemoryFetcher, RemoteNodeFetcher,
    RemoteNodeView, fetch_cached,
};
pub use merge::{AggregationMerge, MergeError, MergeOp, OrderDir, SortKey};
pub use multi_shard_tx::{
    InMemoryShardLockManager, LockError, MultiShardTx, MultiShardTxConfig, MultiShardTxError,
    MultiShardTxMetrics, ShardLockManager, ShardMutator, TxId, TxIdAllocator, WriteSet,
};
pub use plan::{DecomposedPlan, DistributedPlan, Row};
pub use scatter::{
    CoordinatorError, InMemoryShardClient, ScatterGather, ScatterGatherConfig, ShardClient,
    ShardResponse,
};
pub use tcp_client::{
    LeaderCache, ShardRpcRequest, ShardRpcResponse, TcpShardClient, TcpShardClientConfig,
};
