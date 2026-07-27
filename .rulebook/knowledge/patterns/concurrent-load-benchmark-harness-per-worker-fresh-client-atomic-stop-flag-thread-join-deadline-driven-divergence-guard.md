# Concurrent-load benchmark harness: per-worker fresh client, atomic stop flag, thread join, deadline-driven, divergence guard

**Category**: benchmarking
**Tags**: benchmarking, concurrency, rust, latency, throughput

## Description

For a concurrent-load harness layered on top of a synchronous `BenchExecute` trait: spawn N worker threads, each with its own freshly built client (via a `ClientFactory` — never share clients across threads, because client-internal contention would surface as engine concurrency). Coordinate with `Arc<AtomicBool>` stop flag, `Arc<AtomicU64>` total iteration counter, and `Arc<Mutex<Option<HarnessError>>>` for the first error. Drive each worker through a deadline check rather than an iteration count so the wall clock is the only knob. Skip warmup samples by checking `Instant::now() < warmup_until` inside the loop and `continue` on warmup hits. Cross-engine row-count divergence kills every worker via the stop flag. Aggregate per-worker latency vectors after `handle.join()`; compute qps from the cross-worker total iteration count divided by post-warmup wall time.

## Example

pub fn run_concurrent<F: ClientFactory + Sync>(
    scenario: &Scenario, engine_label: &str, factory: &F, cfg: &ConcurrentRunConfig,
) -> Result<ConcurrentResult, HarnessError> {
    let stop = Arc::new(AtomicBool::new(false));
    let total_iters = Arc::new(AtomicU64::new(0));
    let measure_start = Instant::now();
    let warmup_until = measure_start + cfg.warmup;
    let deadline = warmup_until + cfg.duration;
    let handles: Vec<_> = (0..cfg.workers).map(|w| {
        let mut client = factory.build(w)?;
        let stop = Arc::clone(&stop);
        let total = Arc::clone(&total_iters);
        thread::spawn(move || {
            while !stop.load(Relaxed) && Instant::now() < deadline {
                let in_warmup = Instant::now() < warmup_until;
                let t = Instant::now();
                let r = client.execute(&query, timeout)?;
                if !in_warmup {
                    samples.push(t.elapsed());
                    total.fetch_add(1, Relaxed);
                }
            }
        })
    }).collect();
    // park main, join all workers, summarise.
}

## When to Use

When the headline metric is system throughput under concurrency (qps + tail latency vs worker count), not single-call engine latency. Vector DBs, query engines, anything that has lock contention or async I/O under load.

## When NOT to Use

For pure-engine microbenchmarks (use Criterion). For latency-only single-call measurements (use the serial harness — concurrency adds variance).
