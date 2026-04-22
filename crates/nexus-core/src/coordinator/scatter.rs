//! Scatter/gather engine.
//!
//! Takes a [`super::plan::DecomposedPlan`] + a [`ShardClient`] and
//! drives the per-shard RPCs in parallel, applying:
//!
//! * **Timeout**: every RPC is bounded by
//!   [`ScatterGatherConfig::query_timeout`]; timeouts fail the whole
//!   query atomically.
//! * **Leader-hint retry**: `ERR_NOT_LEADER(hint)` triggers up to 3
//!   retries (total) against the new leader.
//! * **Stale-generation retry**: `ERR_STALE_GEN(current_gen)` causes
//!   the engine to re-read cluster metadata via the `refresh_meta`
//!   hook and retry the scatter once.
//! * **Atomicity**: any other shard error aborts the query — partial
//!   rows are dropped and a [`CoordinatorError`] surfaces. The engine
//!   never returns half-results.
//!
//! The [`ShardClient`] trait is synchronous on purpose: async glue
//! lives in `nexus-server` where tokio is already plumbed. The core
//! engine stays testable from pure Rust.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::classify::QueryScope;
use super::merge::{MergeError, merge};
use super::plan::{DecomposedPlan, Row};
use crate::sharding::metadata::{NodeId, ShardId};

/// Per-shard RPC response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShardResponse {
    /// Shard accepted and produced rows.
    Ok { rows: Vec<Row> },
    /// Shard is not the leader — retry against `leader_hint`.
    NotLeader { leader_hint: Option<NodeId> },
    /// Coordinator's cached generation is stale; refresh and retry.
    StaleGeneration { current: u64 },
    /// Shard timed out producing rows.
    ShardTimeout,
    /// Shard-local error, free-form reason.
    ShardError { reason: String },
}

/// Errors the coordinator surfaces to the upper layer.
#[derive(Debug, Error)]
pub enum CoordinatorError {
    /// Query exceeded [`ScatterGatherConfig::query_timeout`].
    #[error("ERR_QUERY_TIMEOUT after {0:?}")]
    QueryTimeout(Duration),
    /// At least one shard failed the query.
    #[error("ERR_SHARD_FAILURE(shard={shard}, reason={reason})")]
    ShardFailure { shard: ShardId, reason: String },
    /// Leader-hint retry budget exhausted.
    #[error("ERR_NOT_LEADER(shard={shard}): retry budget exhausted after {attempts} attempts")]
    NoLeader { shard: ShardId, attempts: usize },
    /// Coordinator's cached metadata is still stale after one refresh.
    #[error("ERR_STALE_GEN persisted after refresh (shard={shard})")]
    StaleGeneration { shard: ShardId },
    /// Merge operator rejected the per-shard rows.
    #[error("ERR_MERGE: {0}")]
    Merge(#[from] MergeError),
}

/// A shard client is whatever object can forward a per-shard RPC and
/// return a [`ShardResponse`]. The production impl wraps the TCP
/// transport; tests use [`InMemoryShardClient`].
pub trait ShardClient: Send + Sync {
    /// Execute `cypher` with `parameters` against the given shard,
    /// honoring `deadline`. Implementations MUST time-bound themselves
    /// — the coordinator also enforces a global timeout but relying on
    /// it alone means slow shards consume full `query_timeout` before
    /// the coordinator can fail.
    fn execute(
        &self,
        shard: ShardId,
        cypher: &str,
        parameters: &serde_json::Map<String, serde_json::Value>,
        generation: u64,
        deadline: Instant,
    ) -> ShardResponse;
}

/// Tunable knobs for scatter/gather.
#[derive(Debug, Clone)]
pub struct ScatterGatherConfig {
    /// Deadline for the whole query.
    pub query_timeout: Duration,
    /// Total retry attempts per shard (including the first) when we
    /// get `NotLeader`. Default 3 per the spec.
    pub max_leader_retries: usize,
    /// Number of stale-generation refreshes allowed per query (just
    /// one — if a second stale-gen fires we bail out).
    pub max_stale_gen_refreshes: usize,
}

impl Default for ScatterGatherConfig {
    fn default() -> Self {
        Self {
            query_timeout: Duration::from_secs(30),
            max_leader_retries: 3,
            max_stale_gen_refreshes: 1,
        }
    }
}

/// Scatter/gather driver.
pub struct ScatterGather {
    cfg: ScatterGatherConfig,
    client: Arc<dyn ShardClient>,
}

impl ScatterGather {
    /// Build a driver around a [`ShardClient`].
    #[must_use]
    pub fn new(cfg: ScatterGatherConfig, client: Arc<dyn ShardClient>) -> Self {
        Self { cfg, client }
    }

    /// Execute a decomposed plan. `num_shards` is the current cluster
    /// shard count (needed to expand `Broadcast`). `generation` is the
    /// coordinator's cached generation number.
    /// `refresh_meta` is invoked once on stale-generation errors to
    /// let the caller re-read metadata from the metadata group; the
    /// returned u64 is the fresh generation.
    pub fn scatter<F>(
        &self,
        plan: DecomposedPlan,
        num_shards: u32,
        generation: u64,
        mut refresh_meta: F,
    ) -> Result<Vec<Row>, CoordinatorError>
    where
        F: FnMut() -> u64,
    {
        let start = Instant::now();
        let deadline = start + self.cfg.query_timeout;

        let targets = expand_scope(&plan.scope, num_shards);

        let mut current_gen = generation;
        let mut stale_refreshes = 0usize;

        loop {
            let per_shard = self.do_scatter(&plan, &targets, current_gen, deadline, start)?;

            // Check for stale-gen — if any response surfaces it, we
            // refresh (once) and retry.
            let stale_shard = per_shard.iter().find_map(|(_, resp)| match resp {
                ShardResponse::StaleGeneration { .. } => Some(()),
                _ => None,
            });
            if stale_shard.is_some() {
                if stale_refreshes >= self.cfg.max_stale_gen_refreshes {
                    return Err(CoordinatorError::StaleGeneration { shard: targets[0] });
                }
                stale_refreshes += 1;
                current_gen = refresh_meta();
                continue;
            }

            // Any terminal shard error fails the whole query.
            for (sid, resp) in &per_shard {
                match resp {
                    ShardResponse::Ok { .. } => {}
                    ShardResponse::NotLeader { .. } => {
                        // Should have been resolved by do_scatter's
                        // retry loop; reaching here means retries
                        // exhausted.
                        return Err(CoordinatorError::NoLeader {
                            shard: *sid,
                            attempts: self.cfg.max_leader_retries,
                        });
                    }
                    ShardResponse::ShardTimeout => {
                        return Err(CoordinatorError::QueryTimeout(self.cfg.query_timeout));
                    }
                    ShardResponse::ShardError { reason } => {
                        return Err(CoordinatorError::ShardFailure {
                            shard: *sid,
                            reason: reason.clone(),
                        });
                    }
                    ShardResponse::StaleGeneration { .. } => {
                        unreachable!("stale-gen was handled above — this branch is unreachable")
                    }
                }
            }

            // All ok — extract rows and merge.
            let rows_per_shard: Vec<Vec<Row>> = per_shard
                .into_iter()
                .map(|(_, resp)| match resp {
                    ShardResponse::Ok { rows } => rows,
                    _ => Vec::new(),
                })
                .collect();

            return merge(&plan.merge, rows_per_shard).map_err(CoordinatorError::from);
        }
    }

    fn do_scatter(
        &self,
        plan: &DecomposedPlan,
        targets: &[ShardId],
        generation: u64,
        deadline: Instant,
        start: Instant,
    ) -> Result<Vec<(ShardId, ShardResponse)>, CoordinatorError> {
        let mut results: Vec<(ShardId, ShardResponse)> = Vec::with_capacity(targets.len());
        for &shard in targets {
            if Instant::now() >= deadline {
                return Err(CoordinatorError::QueryTimeout(
                    Instant::now().saturating_duration_since(start),
                ));
            }
            let resp = self.execute_with_leader_retry(shard, plan, generation, deadline)?;
            results.push((shard, resp));
        }
        Ok(results)
    }

    fn execute_with_leader_retry(
        &self,
        shard: ShardId,
        plan: &DecomposedPlan,
        generation: u64,
        deadline: Instant,
    ) -> Result<ShardResponse, CoordinatorError> {
        let mut attempts = 0;
        loop {
            if Instant::now() >= deadline {
                return Err(CoordinatorError::QueryTimeout(self.cfg.query_timeout));
            }
            attempts += 1;
            let resp = self.client.execute(
                shard,
                &plan.shard_local_cypher,
                &plan.parameters,
                generation,
                deadline,
            );
            match resp {
                ShardResponse::NotLeader { .. } if attempts < self.cfg.max_leader_retries => {
                    // Retry — the client is expected to update its
                    // leader cache from the hint on the next call.
                    continue;
                }
                other => return Ok(other),
            }
        }
    }
}

fn expand_scope(scope: &QueryScope, num_shards: u32) -> Vec<ShardId> {
    match scope {
        QueryScope::SingleShard(s) => vec![*s],
        QueryScope::Targeted(s) => s.clone(),
        QueryScope::Broadcast => (0..num_shards).map(ShardId::new).collect(),
    }
}

/// In-memory client for tests. Routes shard RPCs to per-shard
/// closures. Behavior is controlled by setting responses on each
/// shard slot.
pub struct InMemoryShardClient {
    /// `responses[shard][attempt_idx]` is the response to give on the
    /// `attempt_idx`-th call. If `attempt_idx >= responses[shard].len()`,
    /// the last entry is returned repeatedly.
    responses: Mutex<BTreeMap<ShardId, Vec<ShardResponse>>>,
    /// Per-shard call counters — tests assert on these.
    calls: Mutex<BTreeMap<ShardId, AtomicUsize>>,
    /// Artificial per-shard delay applied before returning. Default 0.
    delays: Mutex<BTreeMap<ShardId, Duration>>,
}

impl InMemoryShardClient {
    /// Empty client — every shard returns `ShardError { reason: "unset" }`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(BTreeMap::new()),
            calls: Mutex::new(BTreeMap::new()),
            delays: Mutex::new(BTreeMap::new()),
        }
    }

    /// Set `responses` for `shard`. The i-th call returns `responses[i]`;
    /// once the vec is exhausted the last entry is repeated.
    pub fn set(&self, shard: ShardId, responses: Vec<ShardResponse>) {
        let mut r = self.responses.lock().expect("mutex poisoned");
        r.insert(shard, responses);
    }

    /// Set a per-shard delay.
    pub fn set_delay(&self, shard: ShardId, delay: Duration) {
        let mut d = self.delays.lock().expect("mutex poisoned");
        d.insert(shard, delay);
    }

    /// How many times `shard` was called.
    #[must_use]
    pub fn call_count(&self, shard: ShardId) -> usize {
        let calls = self.calls.lock().expect("mutex poisoned");
        calls
            .get(&shard)
            .map(|a| a.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
}

impl Default for InMemoryShardClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardClient for InMemoryShardClient {
    fn execute(
        &self,
        shard: ShardId,
        _cypher: &str,
        _parameters: &serde_json::Map<String, serde_json::Value>,
        _generation: u64,
        deadline: Instant,
    ) -> ShardResponse {
        let delay = {
            let d = self.delays.lock().expect("mutex poisoned");
            d.get(&shard).copied().unwrap_or_default()
        };
        if delay > Duration::ZERO {
            // Simulate slow shard but don't block past the deadline.
            let sleep_for = delay.min(deadline.saturating_duration_since(Instant::now()));
            if !sleep_for.is_zero() {
                std::thread::sleep(sleep_for);
            }
            if Instant::now() >= deadline {
                return ShardResponse::ShardTimeout;
            }
        }
        let attempt_idx = {
            let mut c = self.calls.lock().expect("mutex poisoned");
            let counter = c.entry(shard).or_insert_with(|| AtomicUsize::new(0));
            counter.fetch_add(1, Ordering::Relaxed)
        };
        let responses = self.responses.lock().expect("mutex poisoned");
        let queue = responses.get(&shard);
        match queue {
            Some(v) if !v.is_empty() => {
                let idx = attempt_idx.min(v.len() - 1);
                v[idx].clone()
            }
            _ => ShardResponse::ShardError {
                reason: "unset".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::merge::MergeOp;
    use serde_json::Value;

    fn plan(scope: QueryScope, merge: MergeOp) -> DecomposedPlan {
        DecomposedPlan {
            shard_local_cypher: "MATCH (n) RETURN n".into(),
            parameters: Default::default(),
            columns: vec!["n".into()],
            scope,
            merge,
        }
    }

    fn driver(client: Arc<dyn ShardClient>) -> ScatterGather {
        ScatterGather::new(
            ScatterGatherConfig {
                query_timeout: Duration::from_secs(2),
                max_leader_retries: 3,
                max_stale_gen_refreshes: 1,
            },
            client,
        )
    }

    #[test]
    fn single_shard_returns_shard_rows() {
        let client = Arc::new(InMemoryShardClient::new());
        client.set(
            ShardId::new(2),
            vec![ShardResponse::Ok {
                rows: vec![vec![Value::from(7)]],
            }],
        );
        let dr = driver(client);
        let out = dr
            .scatter(
                plan(QueryScope::SingleShard(ShardId::new(2)), MergeOp::Concat),
                4,
                1,
                || 1,
            )
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][0], Value::from(7));
    }

    #[test]
    fn broadcast_concatenates_per_shard_rows() {
        let client = Arc::new(InMemoryShardClient::new());
        for s in 0..3u32 {
            client.set(
                ShardId::new(s),
                vec![ShardResponse::Ok {
                    rows: vec![vec![Value::from(s)]],
                }],
            );
        }
        let dr = driver(client);
        let out = dr
            .scatter(plan(QueryScope::Broadcast, MergeOp::Concat), 3, 1, || 1)
            .unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0][0], Value::from(0));
        assert_eq!(out[2][0], Value::from(2));
    }

    #[test]
    fn not_leader_retries_up_to_limit() {
        let client = Arc::new(InMemoryShardClient::new());
        client.set(
            ShardId::new(0),
            vec![
                ShardResponse::NotLeader {
                    leader_hint: Some(NodeId::new("node-b").unwrap()),
                },
                ShardResponse::NotLeader {
                    leader_hint: Some(NodeId::new("node-c").unwrap()),
                },
                ShardResponse::Ok {
                    rows: vec![vec![Value::from(1)]],
                },
            ],
        );
        let dr = driver(client.clone());
        let out = dr
            .scatter(
                plan(QueryScope::SingleShard(ShardId::new(0)), MergeOp::Concat),
                1,
                1,
                || 1,
            )
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(client.call_count(ShardId::new(0)), 3);
    }

    #[test]
    fn not_leader_exhausts_budget_surfaces_error() {
        let client = Arc::new(InMemoryShardClient::new());
        client.set(
            ShardId::new(0),
            vec![ShardResponse::NotLeader { leader_hint: None }],
        );
        let dr = driver(client);
        let err = dr
            .scatter(
                plan(QueryScope::SingleShard(ShardId::new(0)), MergeOp::Concat),
                1,
                1,
                || 1,
            )
            .unwrap_err();
        assert!(matches!(err, CoordinatorError::NoLeader { .. }));
    }

    #[test]
    fn shard_error_aborts_whole_query_atomically() {
        let client = Arc::new(InMemoryShardClient::new());
        client.set(
            ShardId::new(0),
            vec![ShardResponse::Ok {
                rows: vec![vec![Value::from(1)]],
            }],
        );
        client.set(
            ShardId::new(1),
            vec![ShardResponse::ShardError {
                reason: "disk full".into(),
            }],
        );
        client.set(
            ShardId::new(2),
            vec![ShardResponse::Ok {
                rows: vec![vec![Value::from(2)]],
            }],
        );
        let dr = driver(client);
        let err = dr
            .scatter(plan(QueryScope::Broadcast, MergeOp::Concat), 3, 1, || 1)
            .unwrap_err();
        match err {
            CoordinatorError::ShardFailure { shard, reason } => {
                assert_eq!(shard, ShardId::new(1));
                assert!(reason.contains("disk full"));
            }
            other => panic!("expected ShardFailure, got {other:?}"),
        }
    }

    #[test]
    fn shard_timeout_surfaces_as_query_timeout() {
        let client = Arc::new(InMemoryShardClient::new());
        client.set(ShardId::new(0), vec![ShardResponse::ShardTimeout]);
        let dr = driver(client);
        let err = dr
            .scatter(
                plan(QueryScope::SingleShard(ShardId::new(0)), MergeOp::Concat),
                1,
                1,
                || 1,
            )
            .unwrap_err();
        assert!(matches!(err, CoordinatorError::QueryTimeout(_)));
    }

    #[test]
    fn stale_generation_triggers_one_refresh() {
        let client = Arc::new(InMemoryShardClient::new());
        client.set(
            ShardId::new(0),
            vec![
                ShardResponse::StaleGeneration { current: 5 },
                ShardResponse::Ok {
                    rows: vec![vec![Value::from(42)]],
                },
            ],
        );
        let dr = driver(client);
        let refreshed = std::cell::RefCell::new(0);
        let out = dr
            .scatter(
                plan(QueryScope::SingleShard(ShardId::new(0)), MergeOp::Concat),
                1,
                1,
                || {
                    *refreshed.borrow_mut() += 1;
                    5
                },
            )
            .unwrap();
        assert_eq!(*refreshed.borrow(), 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][0], Value::from(42));
    }

    #[test]
    fn stale_generation_twice_fails() {
        let client = Arc::new(InMemoryShardClient::new());
        client.set(
            ShardId::new(0),
            vec![
                ShardResponse::StaleGeneration { current: 5 },
                ShardResponse::StaleGeneration { current: 6 },
            ],
        );
        let dr = driver(client);
        let err = dr
            .scatter(
                plan(QueryScope::SingleShard(ShardId::new(0)), MergeOp::Concat),
                1,
                1,
                || 5,
            )
            .unwrap_err();
        assert!(matches!(err, CoordinatorError::StaleGeneration { .. }));
    }

    #[test]
    fn broadcast_uses_aggregation_merge() {
        let client = Arc::new(InMemoryShardClient::new());
        // COUNT(*) decomposed — each shard emits its partial count.
        for (s, partial) in [(0u32, 10i64), (1, 20), (2, 12)] {
            client.set(
                ShardId::new(s),
                vec![ShardResponse::Ok {
                    rows: vec![vec![Value::from(partial)]],
                }],
            );
        }
        let merge = MergeOp::Aggregate {
            aggs: vec![super::super::merge::AggregationMerge::Sum { column: 0 }],
        };
        let dr = driver(client);
        let out = dr
            .scatter(plan(QueryScope::Broadcast, merge), 3, 1, || 1)
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][0], Value::from(42));
    }

    #[test]
    fn default_config_values_match_spec() {
        let cfg = ScatterGatherConfig::default();
        assert_eq!(cfg.query_timeout, Duration::from_secs(30));
        assert_eq!(cfg.max_leader_retries, 3);
    }
}
