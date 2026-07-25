//! Sustained throughput measurement for the async WAL writer
//! (WAL-backpressure perf fix, measurement step).
//!
//! `AsyncWalWriter`'s background thread already batches: entries queued via
//! `append` accumulate in memory and are flushed with a SINGLE `sync_all()`
//! fsync per batch, not per entry (`crates/nexus-core/src/wal/async_wal.rs`
//! `flush_batch` -> `writer.rs::Wal::flush`). The hypothesis under test is
//! that the sustained throughput ceiling is therefore
//!
//! ```text
//! entries/s ~= max_batch_size / per_batch_fsync_latency
//! ```
//!
//! i.e. the batch-size policy — not per-entry overhead — is the limiter,
//! and raising `max_batch_size` should raise the ceiling close to linearly
//! (the fixed fsync cost amortizes over more entries) until either disk raw
//! throughput or the fsync floor itself dominates.
//!
//! This binary measures that directly: for each `max_batch_size` in `{100
//! (former default), 500, 1000 (current default), 4000}`, with every other
//! `AsyncWalConfig` field left at its default, it appends
//! `ENTRIES_PER_REP` distinct `WalEntry::CreateRel` entries as fast as
//! possible and then calls `flush()` ONCE to force every queued batch
//! durable. Wall-clock / `ENTRIES_PER_REP` is the true sustained durable
//! throughput — it includes every batch fsync the run incurred, not just
//! the enqueue cost. The writer's own `stats()` snapshot (read once, right
//! after `flush()` returns) is reported alongside so the per-batch latency
//! and batch split behind the headline rate are visible too.
//!
//! Each `max_batch_size` is measured `REPETITIONS` times (a fresh WAL +
//! writer per repetition, exactly like `benches/ingest_relationship.rs`'s
//! `iter_batched` setup — the append/flush pair mutates a stateful WAL
//! file, so it cannot be reused across measurements), and the table below
//! reports the median across repetitions to smooth OS/disk scheduling
//! jitter.
//!
//! criterion was deliberately NOT used: the deliverable is not a timing
//! distribution but a specific per-config report (entries/s alongside avg
//! batch latency, batch-split, and backpressure counters read from
//! `AsyncWalWriter::stats()`), and criterion's `Measurement`/reporter
//! surface has no hook for aggregating and printing arbitrary user
//! counters like `batches_flushed` next to its own timing output. A plain
//! `harness = false` binary (the same declaration mechanism criterion
//! itself relies on) gives full control over what is measured and
//! printed, for a fraction of the ceremony.
//!
//! ## A note on which `AsyncWalStats` field is the real per-batch latency
//!
//! `AsyncWalStats::total_flush_latency_us` reads as "the field to use here",
//! but as of this writing it is declared and snapshotted
//! (`async_wal.rs:64,94`) and never `fetch_add`-ed anywhere in the writer
//! thread — it is permanently `0`. The counter `flush_batch` actually
//! updates on every successful batch is `total_write_latency_us`
//! (`async_wal.rs:642`), timed from `flush_batch`'s entry to right after
//! `wal.flush()` (the `sync_all()` fsync) returns — i.e. it covers the
//! per-entry `wal.append()` buffered-write loop AND the batch's one fsync,
//! not the fsync alone. Per-entry `wal.append()` only writes into the
//! process's file buffer (no syscall-level fsync), so for any batch with
//! more than a handful of entries this sum is fsync-dominated in practice;
//! this bench reports it as "avg batch latency" rather than "avg fsync
//! latency" to stay accurate about what is actually measured. This is a
//! read-only observation about existing instrumentation — this bench does
//! not modify `async_wal.rs`.
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench wal_throughput
//! ```

use nexus_core::testing::TestContext;
use nexus_core::wal::{AsyncWalConfig, AsyncWalStatsSnapshot, AsyncWalWriter, Wal, WalEntry};
use std::time::Instant;

/// WAL entries appended per timed repetition. Large enough that many
/// batches are exercised even at the largest `max_batch_size` under test
/// (4000 -> 12+ batches), so the measured rate reflects the steady-state
/// batch-fsync loop rather than start-up transients.
const ENTRIES_PER_REP: u64 = 50_000;

/// `max_batch_size` values under test. 1000 is `AsyncWalConfig::default()`'s
/// production value (raised from 100 by this fix); 100 is kept as the
/// former-default reference point so the before/after is visible in one run.
const BATCH_SIZES: [usize; 4] = [100, 500, 1000, 4000];

/// Repetitions per `max_batch_size`. The measurement is fsync-bound
/// (dominated by disk/OS behavior, not CPU), so a handful of repetitions —
/// summarized by median below — is enough to smooth scheduling jitter
/// without inflating the run's wall-clock time.
const REPETITIONS: usize = 3;

/// Neo4j-class throughput bar each `max_batch_size` is checked against.
const TARGET_ENTRIES_PER_SEC: f64 = 60_000.0;

/// One repetition's measured throughput plus the writer's self-reported
/// counters, read once via `stats()` immediately after the durability
/// barrier (`flush()`) returns.
#[derive(Debug)]
struct RepResult {
    entries_per_sec: f64,
    avg_batch_latency_us: f64,
    batches_flushed: u64,
    size_batches: u64,
    timeout_batches: u64,
    backpressure_blocks: u64,
    max_queue_depth: u64,
}

/// The reported row for a `max_batch_size`: the per-field median (max, for
/// the `max_queue_depth` watermark) across its `REPETITIONS` samples.
/// `Copy` because every field is a plain numeric — cheap to pass by value
/// into `print_row`.
#[derive(Clone, Copy)]
struct AggregatedRow {
    entries_per_sec: f64,
    avg_batch_latency_us: f64,
    batches_flushed: u64,
    size_batches: u64,
    timeout_batches: u64,
    backpressure_blocks: u64,
    max_queue_depth: u64,
}

/// Build a fresh, self-cleaning `AsyncWalWriter` over a temp WAL file.
/// `AsyncWalConfig` is `default()` with only `max_batch_size` overridden —
/// every other knob (batch age, queue depth, flush interval, channel
/// buffer, checkpoint threshold) stays at its production default, so the
/// sweep isolates the effect of `max_batch_size` alone.
///
/// Returns `(TestContext, AsyncWalWriter)`. Callers MUST bind the context
/// first and the writer second (`let (ctx, writer) = setup_writer(...)`):
/// Rust drops same-`let` bindings in reverse declaration order, so this
/// binding order drops `writer` (which owns the WAL file handle) BEFORE
/// `ctx` removes its temp directory, so the removal succeeds immediately
/// instead of needing `TestContext`'s locked-directory retry queue.
fn setup_writer(max_batch_size: usize) -> (TestContext, AsyncWalWriter) {
    let ctx = TestContext::new();
    let wal_path = ctx.path().join("wal.log");
    let wal = Wal::new(&wal_path).expect("fresh WAL for wal_throughput bench");
    let config = AsyncWalConfig {
        max_batch_size,
        ..AsyncWalConfig::default()
    };
    let writer =
        AsyncWalWriter::new(wal, config).expect("fresh AsyncWalWriter for wal_throughput bench");
    (ctx, writer)
}

/// Build a distinct `WalEntry::CreateRel` for append index `i`. Distinct
/// ids keep the appended stream realistic; the WAL itself does not
/// deduplicate, so this has no bearing on the measured fsync cost.
fn entry_for(i: u64) -> WalEntry {
    WalEntry::CreateRel {
        rel_id: i,
        src: i,
        dst: i + 1,
        type_id: 0,
    }
}

/// Run one timed repetition: append `ENTRIES_PER_REP` entries as fast as
/// possible, call `flush()` once to force every queued batch durable, then
/// read `stats()`. Setup (fresh WAL + writer) is untimed. Tears down the
/// writer and temp WAL directory before returning, and asserts the temp
/// directory is actually gone (no stray WAL dir leak).
fn run_repetition(max_batch_size: usize) -> RepResult {
    let (ctx, writer) = setup_writer(max_batch_size);

    let start = Instant::now();
    for i in 0..ENTRIES_PER_REP {
        writer
            .append(entry_for(i))
            .expect("append must succeed against a healthy writer");
    }
    writer
        .flush()
        .expect("flush must succeed against a healthy writer");
    let elapsed = start.elapsed();

    let stats: AsyncWalStatsSnapshot = writer.stats();
    let entries_per_sec = ENTRIES_PER_REP as f64 / elapsed.as_secs_f64();
    // `total_flush_latency_us` is never populated by the writer thread (see
    // the module doc comment) — `total_write_latency_us` is the field
    // `flush_batch` actually accumulates per batch, covering the
    // `wal.append()` loop plus the batch's one `wal.flush()` fsync.
    let avg_batch_latency_us = if stats.batches_flushed > 0 {
        stats.total_write_latency_us as f64 / stats.batches_flushed as f64
    } else {
        0.0
    };

    let result = RepResult {
        entries_per_sec,
        avg_batch_latency_us,
        batches_flushed: stats.batches_flushed,
        size_batches: stats.size_batches,
        timeout_batches: stats.timeout_batches,
        backpressure_blocks: stats.backpressure_blocks,
        max_queue_depth: stats.max_queue_depth,
    };

    // See `setup_writer` doc comment: writer must drop before ctx.
    drop(writer);
    let temp_dir_path = ctx.path().to_path_buf();
    drop(ctx);
    assert!(
        !temp_dir_path.exists(),
        "wal_throughput bench must not leak its temp WAL directory: {}",
        temp_dir_path.display()
    );

    result
}

/// Median of `values`, sorted in place via `f64::total_cmp` (throughput and
/// latency samples are always finite, never NaN). `values` must be
/// non-empty.
fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// Median of the `u64` field selected by `pick` across `reps`, rounded to
/// the nearest integer.
fn median_u64(reps: &[RepResult], pick: impl Fn(&RepResult) -> u64) -> u64 {
    let mut values: Vec<f64> = reps.iter().map(|r| pick(r) as f64).collect();
    median(&mut values).round() as u64
}

/// Reduce `reps` (one `max_batch_size`'s `REPETITIONS` samples) to the row
/// reported in the table: median for rate/latency/counters, max for the
/// `max_queue_depth` watermark (it is a peak, not a per-run quantity that
/// should be smoothed).
fn aggregate(reps: &[RepResult]) -> AggregatedRow {
    let mut throughputs: Vec<f64> = reps.iter().map(|r| r.entries_per_sec).collect();
    let mut batch_latencies: Vec<f64> = reps.iter().map(|r| r.avg_batch_latency_us).collect();
    AggregatedRow {
        entries_per_sec: median(&mut throughputs),
        avg_batch_latency_us: median(&mut batch_latencies),
        batches_flushed: median_u64(reps, |r| r.batches_flushed),
        size_batches: median_u64(reps, |r| r.size_batches),
        timeout_batches: median_u64(reps, |r| r.timeout_batches),
        backpressure_blocks: median_u64(reps, |r| r.backpressure_blocks),
        max_queue_depth: reps.iter().map(|r| r.max_queue_depth).max().unwrap_or(0),
    }
}

fn print_table_header() {
    println!(
        "WAL async writer sustained throughput ({ENTRIES_PER_REP} entries/rep, \
         {REPETITIONS} reps/config, median reported)"
    );
    println!(
        "{:>15} | {:>12} | {:>18} | {:>9} | {:>13} | {:>16} | {:>13}",
        "max_batch_size",
        "entries/s",
        "avg batch (us)",
        "batches",
        "size-batches",
        "timeout-batches",
        "backpressure"
    );
}

/// Print one table row for `max_batch_size` plus, when the row clears
/// [`TARGET_ENTRIES_PER_SEC`], a verdict line noting the peak queue depth
/// observed for that config.
fn print_row(max_batch_size: usize, row: &AggregatedRow) {
    let AggregatedRow {
        entries_per_sec,
        avg_batch_latency_us,
        batches_flushed,
        size_batches,
        timeout_batches,
        backpressure_blocks,
        max_queue_depth,
    } = *row;
    println!(
        "{max_batch_size:>15} | {entries_per_sec:>12.0} | {avg_batch_latency_us:>18.1} | \
         {batches_flushed:>9} | {size_batches:>13} | {timeout_batches:>16} | \
         {backpressure_blocks:>13}"
    );
    if entries_per_sec >= TARGET_ENTRIES_PER_SEC {
        println!(
            "  -> clears the {TARGET_ENTRIES_PER_SEC:.0} entries/s Neo4j-class bar \
             (max_queue_depth peaked at {max_queue_depth})"
        );
    }
}

fn main() {
    print_table_header();
    for &max_batch_size in &BATCH_SIZES {
        eprintln!(
            "running max_batch_size={max_batch_size} ({REPETITIONS} reps of \
             {ENTRIES_PER_REP} entries)..."
        );
        let reps: Vec<RepResult> = (0..REPETITIONS)
            .map(|_| run_repetition(max_batch_size))
            .collect();
        let row = aggregate(&reps);
        print_row(max_batch_size, &row);
    }
}
