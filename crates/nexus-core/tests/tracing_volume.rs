//! §2.2 of `phase4_tracing-hotpath-hygiene` — default log-volume
//! smoke test.
//!
//! Counts `Event`s at every `tracing::Level` emitted while a full
//! successful query runs through `Executor::execute`. Before the
//! clean-up, a typical query emitted ~10-20 `INFO` lines (`ADVANCED
//! JOIN: …`, `Direct execution optimization used`, `Query cache
//! HIT/stored`, etc.) regardless of the caller. After the cleanup
//! those collapsed to `trace!` and the default filter excludes
//! them — this test pins the budget so regressions show up as
//! compile failures instead of production log spam.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nexus_core::executor::Query;
use nexus_core::testing::create_test_executor;
use tracing::{Level, Subscriber};
use tracing_subscriber::{
    Layer,
    layer::{Context, SubscriberExt},
};

/// Atomic-counter layer that tracks how many events fired at each
/// `Level`. Other layers (spans, fmt) stay out of scope so the
/// assertions below depend only on `on_event`.
#[derive(Default, Clone)]
struct LevelCounter {
    error: Arc<AtomicUsize>,
    warn: Arc<AtomicUsize>,
    info: Arc<AtomicUsize>,
    debug: Arc<AtomicUsize>,
    trace: Arc<AtomicUsize>,
}

impl LevelCounter {
    fn get(&self, level: Level) -> usize {
        let cell = match level {
            Level::ERROR => &self.error,
            Level::WARN => &self.warn,
            Level::INFO => &self.info,
            Level::DEBUG => &self.debug,
            Level::TRACE => &self.trace,
        };
        cell.load(Ordering::SeqCst)
    }
}

impl<S: Subscriber> Layer<S> for LevelCounter {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let cell = match *event.metadata().level() {
            Level::ERROR => &self.error,
            Level::WARN => &self.warn,
            Level::INFO => &self.info,
            Level::DEBUG => &self.debug,
            Level::TRACE => &self.trace,
        };
        cell.fetch_add(1, Ordering::SeqCst);
    }
}

fn run_with_counter<F, R>(body: F) -> (LevelCounter, R)
where
    F: FnOnce() -> R,
{
    let counter = LevelCounter::default();
    let registry = tracing_subscriber::registry().with(counter.clone());
    let result = tracing::subscriber::with_default(registry, body);
    (counter, result)
}

#[test]
fn happy_path_query_stays_within_info_budget() {
    // Whole-process subscriber, so every log call inside
    // Executor::execute funnels through `LevelCounter`.
    let (counter, _res) = run_with_counter(|| {
        let (executor, _ctx) = create_test_executor();

        let query = Query {
            cypher: "RETURN 1 AS x".to_string(),
            params: Default::default(),
        };

        executor
            .execute(&query)
            .expect("trivial RETURN must succeed")
    });

    // Budget: a happy-path query produces zero INFO events and zero
    // WARN / ERROR events. DEBUG / TRACE are allowed but not
    // asserted (they're off by default in production).
    assert_eq!(
        counter.get(Level::INFO),
        0,
        "hot-path INFO should be zero on a happy-path query; \
         got {} — a regression introduced an `info!` in the executor",
        counter.get(Level::INFO)
    );
    assert_eq!(
        counter.get(Level::WARN),
        0,
        "happy-path query must not emit WARN events; got {}",
        counter.get(Level::WARN)
    );
    assert_eq!(
        counter.get(Level::ERROR),
        0,
        "happy-path query must not emit ERROR events; got {}",
        counter.get(Level::ERROR)
    );
}

#[test]
fn match_where_return_stays_within_info_budget() {
    // Slightly richer query — still no hot-path info! expected.
    let (counter, _res) = run_with_counter(|| {
        let (executor, _ctx) = create_test_executor();

        let query = Query {
            cypher: "UNWIND [1, 2, 3] AS x WITH x WHERE x > 1 RETURN x".to_string(),
            params: Default::default(),
        };

        executor
            .execute(&query)
            .expect("UNWIND/WITH/WHERE query must succeed")
    });

    assert_eq!(
        counter.get(Level::INFO),
        0,
        "UNWIND pipeline introduced an INFO log; got {}",
        counter.get(Level::INFO)
    );
    assert_eq!(counter.get(Level::WARN), 0);
    assert_eq!(counter.get(Level::ERROR), 0);
}
