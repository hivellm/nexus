//! Wall-clock benchmarks for the V2 TCP transport bridge.
//!
//! Three measurements that the in-process deterministic harness can't
//! produce:
//!
//! * **`raft_envelope_roundtrip`** — one `RaftEnvelope` sent a → b and
//!   read back on b's inbound channel. Pure wire latency; subtracting
//!   the in-memory `async_roundtrip_through_inmemory_pipe` unit test
//!   from this number gives the TCP overhead.
//! * **`tcp_shard_client_execute`** — one full `ShardClient::execute`
//!   against a minimal echo server on `127.0.0.1`. Covers connect +
//!   rmp-serde encode + write + read + decode. This is the p50
//!   number a benchmark suite would publish for the coordinator's
//!   per-shard RPC cost in V2.
//! * **`three_node_failover_wallclock`** — launches 3 `RaftNode`s over
//!   TCP, waits for leader election, pauses the leader, times how long
//!   a new leader takes. Honours the raft-consensus spec's `≤3 ×
//!   election_timeout_max` bound in wall-clock form.
//!
//! Each benchmark uses Criterion's `iter_custom` loop so we can build
//! the 3-node cluster once per iteration and still capture accurate
//! timing over the scoring region; the cluster teardown cost lives
//! outside the measured window.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use nexus_core::coordinator::{LeaderCache, ShardClient, TcpShardClient, TcpShardClientConfig};
use nexus_core::sharding::metadata::{NodeId, ShardId};
use nexus_core::sharding::raft::types::{LogIndex, RaftEnvelope, RaftMessage, Term, VoteRequest};
use nexus_core::sharding::raft::{
    RaftNode, RaftNodeConfig, RaftTransport, TcpRaftTransport, TcpRaftTransportConfig,
};
use parking_lot::Mutex;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

fn build_runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("tokio runtime")
}

// ---------------------------------------------------------------------------
// Bench 1: single-envelope Raft transport roundtrip
// ---------------------------------------------------------------------------

fn bench_raft_envelope_roundtrip(c: &mut Criterion) {
    let rt = build_runtime();
    c.bench_function("v2/raft_envelope_roundtrip", |b| {
        b.iter_custom(|iters| {
            rt.block_on(async move {
                let cfg = TcpRaftTransportConfig::default();
                let (a, _rx_a) =
                    TcpRaftTransport::start("127.0.0.1:0".parse().unwrap(), cfg.clone())
                        .await
                        .unwrap();
                let (bx, mut rx_b) = TcpRaftTransport::start("127.0.0.1:0".parse().unwrap(), cfg)
                    .await
                    .unwrap();
                a.add_peer(NodeId::new("b").unwrap(), bx.local_addr());
                bx.add_peer(NodeId::new("a").unwrap(), a.local_addr());

                // Warm the connection — first send may pay
                // reconnect/backoff cost.
                let env = RaftEnvelope {
                    shard_id: ShardId::new(0),
                    from: NodeId::new("a").unwrap(),
                    message: RaftMessage::RequestVote(VoteRequest {
                        term: Term(1),
                        candidate: NodeId::new("a").unwrap(),
                        last_log_index: LogIndex::ZERO,
                        last_log_term: Term(0),
                    }),
                };
                let _ = a.send(&NodeId::new("b").unwrap(), env.clone());
                let _ = rx_b.recv().await;

                let start = Instant::now();
                for _ in 0..iters {
                    a.send(&NodeId::new("b").unwrap(), env.clone()).unwrap();
                    let _ = rx_b.recv().await.unwrap();
                }
                let elapsed = start.elapsed();
                a.shutdown();
                bx.shutdown();
                elapsed
            })
        });
    });
}

// ---------------------------------------------------------------------------
// Bench 2: TcpShardClient one-shot execute against a minimal echo server
// ---------------------------------------------------------------------------

async fn spawn_echo_server() -> (std::net::SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            {
                tokio::spawn(async move {
                    // 9-byte header: shard_id(4) + kind(1) + len(4)
                    // Protocol format is internal — we only need to
                    // mirror it enough to bounce a response back.
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut header = [0u8; 9];
                    if stream.read_exact(&mut header).await.is_err() {
                        return;
                    }
                    let shard_id = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
                    let len =
                        u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
                    let mut body = vec![0u8; len + 4];
                    if stream.read_exact(&mut body).await.is_err() {
                        return;
                    }
                    // Extract request id from the rmp-serde payload
                    // — the layout starts with a fixed-field map in
                    // key order (request_id first). A 16-byte
                    // synthetic rmp response works for the bench.
                    //
                    // Actually simpler: deserialize via rmp-serde.
                    #[derive(serde::Deserialize, serde::Serialize)]
                    struct BenchReq {
                        request_id: u64,
                        #[serde(rename = "shard_id")]
                        _shard: u32,
                        #[serde(rename = "generation")]
                        _gen: u64,
                        #[serde(rename = "cypher")]
                        _c: String,
                        #[serde(rename = "parameters")]
                        _p: serde_json::Map<String, serde_json::Value>,
                    }
                    let req: BenchReq = match rmp_serde::from_slice(&body[..len]) {
                        Ok(v) => v,
                        Err(_) => return,
                    };

                    // Build the response payload.
                    use nexus_core::coordinator::{ShardResponse, ShardRpcResponse};
                    let resp = ShardRpcResponse {
                        request_id: req.request_id,
                        payload: ShardResponse::Ok { rows: vec![] },
                    };
                    let payload = rmp_serde::to_vec_named(&resp).expect("encode");
                    let mut out = Vec::with_capacity(9 + payload.len() + 4);
                    out.extend_from_slice(&shard_id.to_le_bytes());
                    out.push(0x61); // SHARD_RPC_RESPONSE
                    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                    out.extend_from_slice(&payload);
                    let mut h = crc32fast::Hasher::new();
                    h.update(&out);
                    out.extend_from_slice(&h.finalize().to_le_bytes());
                    let _ = stream.write_all(&out).await;
                    let _ = stream.flush().await;
                });
            }
        }
    });
    (addr, handle)
}

fn bench_tcp_shard_client_execute(c: &mut Criterion) {
    let rt = build_runtime();
    c.bench_function("v2/tcp_shard_client_execute", |b| {
        b.iter_custom(|iters| {
            let handle = rt.handle().clone();
            rt.block_on(async move {
                let (addr, server) = spawn_echo_server().await;
                let mut addrs = BTreeMap::new();
                addrs.insert(NodeId::new("leader").unwrap(), addr);
                let mut members = BTreeMap::new();
                members.insert(ShardId::new(0), vec![NodeId::new("leader").unwrap()]);
                let client = Arc::new(TcpShardClient::new(
                    TcpShardClientConfig::default(),
                    handle,
                    addrs,
                    members,
                    Arc::new(LeaderCache::new()),
                ));

                let start = Instant::now();
                for _ in 0..iters {
                    let c = client.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        c.execute(
                            ShardId::new(0),
                            "RETURN 1",
                            &serde_json::Map::new(),
                            1,
                            Instant::now() + Duration::from_secs(2),
                        )
                    })
                    .await
                    .unwrap();
                }
                let elapsed = start.elapsed();
                server.abort();
                elapsed
            })
        });
    });
}

// ---------------------------------------------------------------------------
// Bench 3: three-node failover wall-clock
// ---------------------------------------------------------------------------

struct BenchReplica {
    id: NodeId,
    transport: Arc<TcpRaftTransport>,
    node: Arc<Mutex<RaftNode>>,
    paused: Arc<Mutex<bool>>,
    driver: JoinHandle<()>,
}

impl Drop for BenchReplica {
    fn drop(&mut self) {
        self.driver.abort();
        self.transport.shutdown();
    }
}

async fn spawn_bench_replica(id: NodeId, members: Vec<NodeId>, rng_seed: u64) -> BenchReplica {
    let (transport, mut inbound_rx) = TcpRaftTransport::start(
        "127.0.0.1:0".parse().unwrap(),
        TcpRaftTransportConfig::default(),
    )
    .await
    .unwrap();
    let transport = Arc::new(transport);
    let cfg = RaftNodeConfig {
        shard_id: ShardId::new(0),
        node_id: id.clone(),
        members,
        election_timeout_min: Duration::from_millis(150),
        election_timeout_max: Duration::from_millis(300),
        heartbeat_interval: Duration::from_millis(50),
        tick: Duration::from_millis(10),
        rng_seed,
    };
    let node = Arc::new(Mutex::new(RaftNode::new(cfg).unwrap()));
    let paused = Arc::new(Mutex::new(false));
    let transport_for_driver = transport.clone();
    let node_for_driver = node.clone();
    let paused_for_driver = paused.clone();
    let driver = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(10));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                Some(env) = inbound_rx.recv() => {
                    if *paused_for_driver.lock() { continue; }
                    let mut n = node_for_driver.lock();
                    let _ = n.handle_message(env, transport_for_driver.as_ref());
                }
                _ = ticker.tick() => {
                    if *paused_for_driver.lock() { continue; }
                    let mut n = node_for_driver.lock();
                    let _ = n.tick(transport_for_driver.as_ref());
                }
            }
        }
    });
    BenchReplica {
        id,
        transport,
        node,
        paused,
        driver,
    }
}

fn bench_three_node_failover(c: &mut Criterion) {
    let rt = build_runtime();
    let mut group = c.benchmark_group("v2/three_node_failover");
    // Each iteration spins a fresh 3-node cluster, so limit sample
    // size to keep the wall-clock runtime sane.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(15));
    group.bench_function("wallclock", |b| {
        b.iter_custom(|iters| {
            rt.block_on(async move {
                let mut total = Duration::ZERO;
                for iter in 0..iters {
                    let ids: Vec<NodeId> = ["a", "b", "c"]
                        .iter()
                        .map(|s| NodeId::new(*s).unwrap())
                        .collect();
                    let mut replicas = Vec::with_capacity(3);
                    for (i, id) in ids.iter().enumerate() {
                        replicas.push(
                            spawn_bench_replica(
                                id.clone(),
                                ids.clone(),
                                iter.wrapping_mul(7).wrapping_add(i as u64 + 1),
                            )
                            .await,
                        );
                    }
                    // Wire peers.
                    let addrs: BTreeMap<_, _> = replicas
                        .iter()
                        .map(|r| (r.id.clone(), r.transport.local_addr()))
                        .collect();
                    for r in &replicas {
                        for (pid, paddr) in &addrs {
                            if pid != &r.id {
                                r.transport.add_peer(pid.clone(), *paddr);
                            }
                        }
                    }

                    // Wait for initial election.
                    let deadline = Instant::now() + Duration::from_secs(3);
                    let mut old_leader = None;
                    while Instant::now() < deadline {
                        if let Some(lid) = single_leader(&replicas) {
                            old_leader = Some(lid);
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    let Some(old_leader) = old_leader else {
                        continue;
                    };

                    // Pause the old leader and time the failover.
                    let old_r = replicas.iter().find(|r| r.id == old_leader).unwrap();
                    *old_r.paused.lock() = true;

                    let start = Instant::now();
                    let deadline = start + Duration::from_secs(3);
                    while Instant::now() < deadline {
                        match single_leader(&replicas) {
                            Some(lid) if lid != old_leader => break,
                            _ => tokio::time::sleep(Duration::from_millis(10)).await,
                        }
                    }
                    total += start.elapsed();
                }
                total
            })
        });
    });
    group.finish();
}

fn single_leader(replicas: &[BenchReplica]) -> Option<NodeId> {
    let live: Vec<(NodeId, Term)> = replicas
        .iter()
        .filter(|r| !*r.paused.lock())
        .filter_map(|r| {
            let n = r.node.lock();
            if n.is_leader() {
                Some((r.id.clone(), n.current_term()))
            } else {
                None
            }
        })
        .collect();
    if live.is_empty() {
        return None;
    }
    let max_term = live.iter().map(|(_, t)| *t).max().unwrap();
    let at_max: Vec<_> = live.into_iter().filter(|(_, t)| *t == max_term).collect();
    if at_max.len() == 1 {
        Some(at_max[0].0.clone())
    } else {
        None
    }
}

criterion_group!(
    benches,
    bench_raft_envelope_roundtrip,
    bench_tcp_shard_client_execute,
    bench_three_node_failover
);
criterion_main!(benches);
