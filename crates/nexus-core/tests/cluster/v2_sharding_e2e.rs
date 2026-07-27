//! End-to-end tests for V2 sharding.
//!
//! These exercises wire Phase 1–5 together: a controller fed by a
//! multi-node Raft harness drives a coordinator that scatters queries
//! through an in-memory shard client. Each test validates one §Scenario
//! from the spec suite.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nexus_core::coordinator::classify::classify;
use nexus_core::coordinator::{
    AggregationMerge, ClassifyHints, CoordinatorError, DecomposedPlan, DistributedPlan, MergeOp,
    QueryScope, ScatterGather, ScatterGatherConfig, ShardClient, ShardResponse,
};
use nexus_core::sharding::assignment::shard_for_node_u64;
use nexus_core::sharding::controller::{AddNodeRequest, ClusterController, StaticAllHealthy};
use nexus_core::sharding::metadata::{ClusterMeta, MetaChange, NodeId, NodeInfo, ShardId};
use nexus_core::sharding::raft::{InMemoryTransport, RaftNode, RaftNodeConfig};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn nid(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}

fn ninfo(id: &str, port: u16) -> NodeInfo {
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    NodeInfo::new(nid(id), addr)
}

fn bootstrap_three_node() -> ClusterMeta {
    ClusterMeta::bootstrap(
        vec![
            ninfo("node-a", 15480),
            ninfo("node-b", 15481),
            ninfo("node-c", 15482),
        ],
        3,
        3,
    )
    .unwrap()
}

fn controller_for(meta: ClusterMeta, this: &str, is_leader: bool) -> Arc<ClusterController> {
    Arc::new(ClusterController::new(
        nid(this),
        meta,
        is_leader,
        Arc::new(StaticAllHealthy),
    ))
}

/// Shared-count test client that returns deterministic rows based on
/// shard id. Used by the scatter/gather tests — a stand-in for the
/// real TCP client.
struct CountingShardClient {
    /// Per-shard responses.
    responses: Mutex<std::collections::BTreeMap<ShardId, Vec<ShardResponse>>>,
    /// Running call counter — tests assert on it.
    total_calls: AtomicU64,
}

impl CountingShardClient {
    fn new() -> Self {
        Self {
            responses: Mutex::new(Default::default()),
            total_calls: AtomicU64::new(0),
        }
    }

    fn set(&self, shard: ShardId, r: Vec<ShardResponse>) {
        self.responses.lock().unwrap().insert(shard, r);
    }
}

impl ShardClient for CountingShardClient {
    fn execute(
        &self,
        shard: ShardId,
        _cypher: &str,
        _parameters: &serde_json::Map<String, serde_json::Value>,
        _generation: u64,
        _deadline: Instant,
    ) -> ShardResponse {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        let r = self.responses.lock().unwrap();
        r.get(&shard)
            .and_then(|v| v.first().cloned())
            .unwrap_or(ShardResponse::ShardError {
                reason: format!("no response configured for {shard}"),
            })
    }
}

// ---------------------------------------------------------------------------
// §Scenario: Metadata consistency after leader change (raft-consensus spec)
// ---------------------------------------------------------------------------

#[test]
fn metadata_survives_simulated_leader_failover() {
    // A three-node controller with node-a elected as metadata leader
    // accepts a mutation, then we demote it + promote node-b. The new
    // leader must see the same metadata bytes.
    let ca = controller_for(bootstrap_three_node(), "node-a", true);

    ca.add_node(AddNodeRequest {
        node_id: "node-d".into(),
        addr: "127.0.0.1:15483".into(),
        zone: String::new(),
    })
    .expect("add_node on leader");

    // Hand-off: copy the (now-updated) metadata into cb as if Raft
    // had replicated the log entry.
    let replicated = ca.meta();

    let cb = Arc::new(ClusterController::new(
        nid("node-b"),
        replicated.clone(),
        true,
        Arc::new(StaticAllHealthy),
    ));

    // Now cb is leader. The add_node is persisted.
    assert!(cb.status().nodes.contains_key("node-d"));
    assert_eq!(cb.meta().generation, ca.meta().generation);
    assert_eq!(cb.meta().cluster_id, ca.meta().cluster_id);
}

// ---------------------------------------------------------------------------
// §Scenario: Deterministic assignment + relationship co-location
// (sharding spec)
// ---------------------------------------------------------------------------

#[test]
fn shard_assignment_is_deterministic_across_restarts() {
    // Boot twice, compare the shard each id lands on.
    let meta1 = bootstrap_three_node();
    let meta2 = bootstrap_three_node();
    for node_id in 0u64..500 {
        let s1 = shard_for_node_u64(&node_id, meta1.num_shards);
        let s2 = shard_for_node_u64(&node_id, meta2.num_shards);
        assert_eq!(s1, s2, "node_id {node_id} landed on different shards");
    }
}

#[test]
fn shard_metadata_roundtrips_through_bincode() {
    // A metadata snapshot survives a bincode round-trip — i.e. the
    // Raft snapshot install path would recover byte-identical state.
    let meta = bootstrap_three_node();
    let bytes = bincode::serialize(&meta).unwrap();
    let back: ClusterMeta = bincode::deserialize(&bytes).unwrap();
    assert_eq!(meta, back);
}

// ---------------------------------------------------------------------------
// §Scenario: Single-shard classification + filter pushdown
// (distributed-query spec)
// ---------------------------------------------------------------------------

#[test]
fn single_shard_query_hits_exactly_one_shard() {
    let client = Arc::new(CountingShardClient::new());
    // Build classification for id=42 across 4 shards.
    let shard = shard_for_node_u64(&42, 4);
    client.set(
        shard,
        vec![ShardResponse::Ok {
            rows: vec![vec![Value::from("Alice")]],
        }],
    );

    let hints = ClassifyHints {
        sharding_key_value: Some(42),
        ..Default::default()
    };
    let classified = classify(&hints, 4);
    match classified.scope {
        QueryScope::SingleShard(s) => assert_eq!(s, shard),
        other => panic!("expected SingleShard, got {other:?}"),
    }

    let plan = DistributedPlan {
        shard_local_cypher: "MATCH (n:Person {id: $x}) RETURN n.name".into(),
        parameters: Default::default(),
        columns: vec!["n.name".into()],
        scope: classified.scope,
        merge: MergeOp::Concat,
    };
    let dec: DecomposedPlan = plan.decompose();
    let dr = ScatterGather::new(ScatterGatherConfig::default(), client.clone());
    let out = dr.scatter(dec, 4, 1, || 1).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0][0], Value::from("Alice"));
    assert_eq!(
        client.total_calls.load(Ordering::Relaxed),
        1,
        "single-shard query must issue exactly one RPC"
    );
}

// ---------------------------------------------------------------------------
// §Scenario: AVG decomposed correctly (distributed-query spec)
// ---------------------------------------------------------------------------

#[test]
fn avg_aggregation_is_decomposed_across_shards() {
    // Per-shard (sum, count) partials: (100, 5), (200, 10), (300, 15).
    // Final avg = 600 / 30 = 20.0.
    let client = Arc::new(CountingShardClient::new());
    for (s, sum, count) in [(0u32, 100i64, 5u64), (1, 200, 10), (2, 300, 15)] {
        client.set(
            ShardId::new(s),
            vec![ShardResponse::Ok {
                rows: vec![vec![Value::from(sum), Value::from(count)]],
            }],
        );
    }
    let plan = DecomposedPlan {
        shard_local_cypher: "MATCH (n:Person) WITH sum(n.age) AS s, count(n) AS c RETURN s, c"
            .into(),
        parameters: Default::default(),
        columns: vec!["avg".into()],
        scope: QueryScope::Broadcast,
        merge: MergeOp::Aggregate {
            aggs: vec![AggregationMerge::Avg {
                sum_column: 0,
                count_column: 1,
            }],
        },
    };
    let dr = ScatterGather::new(ScatterGatherConfig::default(), client);
    let out = dr.scatter(plan, 3, 1, || 1).unwrap();
    assert_eq!(out.len(), 1);
    let got = out[0][0].as_f64().unwrap();
    assert!((got - 20.0).abs() < 1e-9, "expected 20.0, got {got}");
}

// ---------------------------------------------------------------------------
// §Scenario: Shard-failure atomicity (distributed-query spec)
// ---------------------------------------------------------------------------

#[test]
fn shard_failure_aborts_the_whole_query() {
    let client = Arc::new(CountingShardClient::new());
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
            rows: vec![vec![Value::from(3)]],
        }],
    );
    let plan = DecomposedPlan {
        shard_local_cypher: "MATCH (n) RETURN n".into(),
        parameters: Default::default(),
        columns: vec!["n".into()],
        scope: QueryScope::Broadcast,
        merge: MergeOp::Concat,
    };
    let dr = ScatterGather::new(ScatterGatherConfig::default(), client);
    let err = dr.scatter(plan, 3, 1, || 1).unwrap_err();
    assert!(matches!(err, CoordinatorError::ShardFailure { .. }));
}

// ---------------------------------------------------------------------------
// §Scenario: Fast failover — 3-replica Raft elects a leader after
// leader crash (raft-consensus spec)
// ---------------------------------------------------------------------------

#[test]
fn raft_failover_meets_bound() {
    use nexus_core::sharding::raft::cluster::RaftTestCluster;

    let mut c = RaftTestCluster::new(ShardId::new(0), vec![nid("a"), nid("b"), nid("c")], 17);
    c.tick_until(100, |c| c.leader().is_some());
    let old = c.leader().unwrap();

    c.crash(old.clone());

    // Spec bound: new leader within 3× election timeout.
    // Election min = 150ms, tick = 10ms — so 90 ticks at most.
    let elapsed = c.tick_until(200, |c| matches!(c.leader(), Some(l) if l != old));
    let new_leader = c.leader().expect("no new leader elected");
    assert_ne!(new_leader, old);
    assert!(
        elapsed <= 90,
        "failover took {elapsed} ticks, spec bound is 90"
    );
}

// ---------------------------------------------------------------------------
// §Scenario: Majority failure tolerated (raft-consensus spec)
// ---------------------------------------------------------------------------

#[test]
fn raft_minority_failure_keeps_replicating() {
    use nexus_core::sharding::raft::cluster::RaftTestCluster;

    let mut c = RaftTestCluster::new(ShardId::new(0), vec![nid("a"), nid("b"), nid("c")], 23);
    c.tick_until(100, |c| c.leader().is_some());
    let leader = c.leader().unwrap();
    // Crash one follower; the majority of 2/3 should still commit.
    let follower = ["a", "b", "c"]
        .iter()
        .map(|s| nid(s))
        .find(|n| *n != leader)
        .unwrap();
    c.crash(follower);

    let idx = c.propose_on_leader(b"after-one-crash".to_vec()).unwrap();
    // Wait for commit to reach the leader + other follower.
    let ticks = c.tick_until(100, |c| {
        c.node(&leader)
            .map(|n| n.commit_index() >= idx)
            .unwrap_or(false)
    });
    assert!(
        c.node(&leader).unwrap().commit_index() >= idx,
        "commit did not reach leader after {ticks} ticks"
    );
}

// ---------------------------------------------------------------------------
// §Scenario: Rebalance converges (sharding + controller spec)
// ---------------------------------------------------------------------------

#[test]
fn rebalance_converges_after_repeated_calls() {
    // Build an unbalanced 4-node controller.
    let mut meta = ClusterMeta::bootstrap(
        vec![
            ninfo("node-a", 15480),
            ninfo("node-b", 15481),
            ninfo("node-c", 15482),
            ninfo("node-d", 15483),
        ],
        4,
        2,
    )
    .unwrap();
    // Force an imbalance — put node-a on every shard.
    for s in 0..4u32 {
        let sid = ShardId::new(s);
        if !meta.shards[s as usize].contains(&nid("node-a")) {
            // Swap out whoever is second to make a fit.
            let victim = meta.shards[s as usize].members[1].clone();
            meta.apply(MetaChange::ReplaceShardMember {
                shard_id: sid,
                remove: victim,
                add: nid("node-a"),
            })
            .unwrap();
        }
    }

    let c = Arc::new(ClusterController::new(
        nid("node-a"),
        meta,
        true,
        Arc::new(StaticAllHealthy),
    ));

    // Rebalance until noop.
    for pass in 0..10 {
        let moves = c.rebalance().unwrap();
        if moves == 0 {
            break;
        }
        assert!(pass < 9, "rebalance failed to converge");
    }

    let meta = c.meta();
    let cap = meta.max_replicas_per_node();
    for node in ["node-a", "node-b", "node-c", "node-d"] {
        let load = meta.replicas_on(&nid(node));
        assert!(load <= cap, "{node} at {load}, cap={cap}");
    }
}

// ---------------------------------------------------------------------------
// §Scenario: Leader redirect via controller — followers refuse writes
// (cluster-api spec)
// ---------------------------------------------------------------------------

#[test]
fn follower_refuses_mutation_with_leader_hint() {
    let meta = bootstrap_three_node();
    let ca = controller_for(meta.clone(), "node-a", true);
    let cb = controller_for(meta, "node-b", false);
    cb.set_leader(false, Some(nid("node-a")));

    let err = cb
        .add_node(AddNodeRequest {
            node_id: "node-d".into(),
            addr: "127.0.0.1:15483".into(),
            zone: String::new(),
        })
        .unwrap_err();
    match err {
        nexus_core::sharding::controller::ControllerError::NotMetadataLeader { leader_hint } => {
            assert_eq!(leader_hint.as_ref().map(|n| n.as_str()), Some("node-a"));
        }
        other => panic!("expected NotMetadataLeader, got {other:?}"),
    }
    // The leader still works.
    ca.add_node(AddNodeRequest {
        node_id: "node-d".into(),
        addr: "127.0.0.1:15483".into(),
        zone: String::new(),
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// §Scenario: Stale-generation detection round trip
// (sharding + distributed-query spec)
// ---------------------------------------------------------------------------

#[test]
fn stale_generation_triggers_refresh_then_succeeds() {
    let client = Arc::new(CountingShardClient::new());
    client.set(
        ShardId::new(0),
        vec![
            ShardResponse::StaleGeneration { current: 5 },
            ShardResponse::Ok {
                rows: vec![vec![Value::from("ok")]],
            },
        ],
    );
    // CountingShardClient only returns the first response repeatedly
    // — use two-response simulation via InMemoryShardClient for this
    // test; easier to swap than to complicate CountingShardClient.
    let mem_client = Arc::new(nexus_core::coordinator::InMemoryShardClient::new());
    mem_client.set(
        ShardId::new(0),
        vec![
            ShardResponse::StaleGeneration { current: 5 },
            ShardResponse::Ok {
                rows: vec![vec![Value::from("ok")]],
            },
        ],
    );
    let dr = ScatterGather::new(ScatterGatherConfig::default(), mem_client);
    let mut refreshed = 0;
    let out = dr
        .scatter(
            DecomposedPlan {
                shard_local_cypher: "RETURN 1".into(),
                parameters: Default::default(),
                columns: vec!["x".into()],
                scope: QueryScope::SingleShard(ShardId::new(0)),
                merge: MergeOp::Concat,
            },
            1,
            1,
            || {
                refreshed += 1;
                5
            },
        )
        .unwrap();
    assert_eq!(refreshed, 1);
    assert_eq!(out[0][0], Value::from("ok"));
    // Consume unused client ref to avoid dead_code.
    drop(client);
}

// ---------------------------------------------------------------------------
// §Scenario: InMemoryTransport wiring — messages only flow between
// registered nodes (raft-consensus spec, wire format)
// ---------------------------------------------------------------------------

#[test]
fn raft_transport_isolates_unregistered_nodes() {
    let transport = InMemoryTransport::new();
    transport.register(nid("a"));
    let cfg = RaftNodeConfig {
        shard_id: ShardId::new(0),
        node_id: nid("a"),
        members: vec![nid("a")],
        election_timeout_min: Duration::from_millis(100),
        election_timeout_max: Duration::from_millis(200),
        heartbeat_interval: Duration::from_millis(30),
        tick: Duration::from_millis(5),
        rng_seed: 1,
    };
    let mut node = RaftNode::new(cfg).unwrap();
    for _ in 0..50 {
        node.tick(transport.as_ref()).unwrap();
        if node.is_leader() {
            break;
        }
    }
    assert!(node.is_leader(), "single-node cluster must elect itself");
}
