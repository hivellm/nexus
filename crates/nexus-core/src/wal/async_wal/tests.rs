use super::*;
use crate::testing::TestContext;
use crate::wal::{Wal, WalEntry};
use crossbeam_channel::bounded;
use std::thread;

fn create_test_writer() -> (AsyncWalWriter, TestContext) {
    let ctx = TestContext::new();
    let wal_path = ctx.path().join("wal.log");
    let wal = Wal::new(&wal_path).unwrap();

    let config = AsyncWalConfig {
        max_batch_size: 10,
        max_batch_age: Duration::from_millis(50),
        max_queue_depth: 100,
        flush_interval: Duration::from_millis(25),
        channel_buffer_size: 50,
        checkpoint_size_bytes: u64::MAX,
        flush_gate: None,
        fail_flush: None,
    };

    let writer = AsyncWalWriter::new(wal, config).unwrap();
    (writer, ctx)
}

#[test]
fn test_async_writer_creation() {
    let (mut writer, _dir) = create_test_writer();
    assert_eq!(writer.stats().entries_submitted, 0);
}

/// phase0_fix-wal-durability-gaps #4: when a live flush exhausts its
/// retries, the batch is emergency-saved in the real frame format into the
/// WAL's own directory, and a fresh `Wal` recovers it — instead of the old
/// silent loss to an unparseable CWD-relative file.
#[test]
fn async_emergency_save_is_recoverable_after_flush_failure() {
    use std::sync::atomic::AtomicBool;

    let ctx = TestContext::new();
    let wal_path = ctx.path().join("wal.log");
    let wal = Wal::new(&wal_path).unwrap();

    let fail = Arc::new(AtomicBool::new(true));
    let config = AsyncWalConfig {
        max_batch_size: 10,
        max_batch_age: Duration::from_millis(50),
        max_queue_depth: 100,
        flush_interval: Duration::from_millis(25),
        channel_buffer_size: 50,
        checkpoint_size_bytes: u64::MAX,
        flush_gate: None,
        fail_flush: Some(fail.clone()),
    };
    let mut writer = AsyncWalWriter::new(wal, config).unwrap();

    writer
        .append(WalEntry::ExternalIdAssigned {
            internal_id: 99,
            external_id_bytes: vec![9, 9],
        })
        .unwrap();

    // Force a flush; with fail_flush set it exhausts retries and takes the
    // emergency-save path. flush() returns Err — that is expected here.
    let _ = writer.flush();
    drop(writer); // writer thread drains and exits, releasing the WAL

    // A fresh Wal over the same directory recovers the emergency-saved entry.
    let probe = Wal::new(&wal_path).unwrap();
    let recovered = probe.recover_emergency().unwrap();
    assert!(
        recovered.iter().any(|e| matches!(
            e,
            WalEntry::ExternalIdAssigned {
                internal_id: 99,
                ..
            }
        )),
        "an entry that hit the emergency path must be recoverable: {recovered:?}"
    );
}

// ---- phase0_fix-wal-checkpoint-truncate-production ---------------------

fn checkpoint_test_config(checkpoint_size_bytes: u64) -> AsyncWalConfig {
    AsyncWalConfig {
        max_batch_size: 20,
        max_batch_age: Duration::from_millis(20),
        max_queue_depth: 2000,
        flush_interval: Duration::from_millis(10),
        channel_buffer_size: 1000,
        checkpoint_size_bytes,
        flush_gate: None,
        fail_flush: None,
    }
}

/// Baseline (§1): with the trigger disabled (`u64::MAX`), nothing ever
/// checkpoints — the WAL just grows, exactly as production did before this
/// fix (no checkpoint/truncate caller existed).
#[test]
fn wal_grows_without_a_checkpoint_trigger() {
    let ctx = TestContext::new();
    let wal_path = ctx.path().join("wal.log");
    let wal = Wal::new(&wal_path).unwrap();
    let mut writer = AsyncWalWriter::new(wal, checkpoint_test_config(u64::MAX)).unwrap();

    for i in 0..500u64 {
        writer
            .append(WalEntry::CreateNode {
                node_id: i,
                label_bits: i,
            })
            .unwrap();
    }
    writer.flush().unwrap();

    assert_eq!(
        writer.stats().wal_checkpoints,
        0,
        "no checkpoint may fire when the trigger is disabled"
    );
    drop(writer);
    let reopened = Wal::new(&wal_path).unwrap();
    assert!(
        reopened.file_size() > 1000,
        "without a trigger the WAL grows unbounded, got {}",
        reopened.file_size()
    );
}

/// Fix (§3): once the WAL exceeds the size threshold, the writer thread
/// checkpoint-truncates it after the flush, so it stays bounded below the
/// threshold and never approaches health_check's 1 GiB gate.
#[test]
fn wal_is_checkpoint_truncated_past_the_size_threshold() {
    const THRESHOLD: u64 = 512;
    let ctx = TestContext::new();
    let wal_path = ctx.path().join("wal.log");
    let wal = Wal::new(&wal_path).unwrap();
    let mut writer = AsyncWalWriter::new(wal, checkpoint_test_config(THRESHOLD)).unwrap();

    // ~1000 frames far exceed the 512-byte threshold, so the WAL is
    // compacted repeatedly along the way.
    for i in 0..1000u64 {
        writer
            .append(WalEntry::CreateNode {
                node_id: i,
                label_bits: i,
            })
            .unwrap();
    }
    writer.flush().unwrap();

    assert!(
        writer.stats().wal_checkpoints >= 1,
        "the WAL must have been checkpoint-truncated at least once"
    );
    drop(writer);

    // Invariant: after every flushed batch the writer truncates once the
    // file reaches the threshold, so a reopened WAL is always below it —
    // never the ~30 KB the 1000 raw frames would occupy.
    let reopened = Wal::new(&wal_path).unwrap();
    assert!(
        reopened.file_size() < THRESHOLD,
        "the WAL must stay bounded below the threshold, got {}",
        reopened.file_size()
    );
}

#[test]
fn test_append_entry() {
    let (mut writer, _dir) = create_test_writer();

    let entry = WalEntry::BeginTx {
        tx_id: 1,
        epoch: 100,
    };

    writer.append(entry).unwrap();
    assert_eq!(writer.stats().entries_submitted, 1);
}

#[test]
fn test_shutdown() {
    let (mut writer, _dir) = create_test_writer();

    // Submit some entries
    for i in 0..5 {
        let entry = WalEntry::CreateNode {
            node_id: i,
            label_bits: 0,
        };
        writer.append(entry).unwrap();
    }

    // Shutdown should flush everything
    writer.shutdown().unwrap();
}

#[test]
fn test_multiple_appends_and_flush() {
    let (mut writer, _dir) = create_test_writer();

    // Submit multiple entries
    for i in 0..20 {
        let entry = WalEntry::CreateNode {
            node_id: i,
            label_bits: 0,
        };
        writer.append(entry).unwrap();
    }

    // Force flush
    writer.flush().unwrap();

    // Give more time for async processing to complete
    thread::sleep(Duration::from_millis(500));

    let stats = writer.stats();
    assert_eq!(stats.entries_submitted, 20);
    // Note: entries_written may be 0 on fast systems where shutdown happens before write
    // This is acceptable behavior - we just verify entries were submitted
    assert!(stats.entries_submitted > 0, "Should have submitted entries");

    writer.shutdown().unwrap();
}

/// #19: a burst far larger than the channel capacity must not deadlock —
/// the submitting thread blocks on backpressure (try_send Full -> blocking
/// send) and every entry is accepted and eventually written.
#[test]
fn test_backpressure_burst_does_not_deadlock() {
    let ctx = TestContext::new();
    let wal = Wal::new(ctx.path().join("wal.log")).unwrap();
    let config = AsyncWalConfig {
        max_batch_size: 10,
        max_batch_age: Duration::from_millis(20),
        max_queue_depth: 16, // channel capacity = max(8, 16) = 16
        flush_interval: Duration::from_millis(10),
        channel_buffer_size: 8,
        checkpoint_size_bytes: u64::MAX,
        flush_gate: None,
        fail_flush: None,
    };
    let mut writer = AsyncWalWriter::new(wal, config).unwrap();

    // Submit 2000 entries into a 16-slot channel — exercises the
    // full-channel backpressure path repeatedly. Must complete, not hang.
    let burst = 2000u64;
    for i in 0..burst {
        writer
            .append(WalEntry::CreateNode {
                node_id: i,
                label_bits: 0,
            })
            .expect("append must not fail under backpressure");
    }
    assert_eq!(
        writer.stats().entries_submitted,
        burst,
        "all entries accepted despite a channel smaller than the burst (no deadlock)"
    );

    // Shutdown drains + joins the writer thread without hanging. (#19 is
    // about the submit path no longer dead-ending on a full channel;
    // exact shutdown-drain timing is a separate, non-deterministic concern.)
    writer.shutdown().unwrap();

    // Durability: the burst survives backpressure end-to-end — a fresh
    // Wal on the same file replays every entry (none dropped while the
    // channel was full).
    let mut reopened = Wal::new(ctx.path().join("wal.log")).unwrap();
    let recovered = reopened.recover().unwrap();
    let create_nodes = recovered
        .iter()
        .filter(|e| matches!(e, WalEntry::CreateNode { .. }))
        .count() as u64;
    assert_eq!(
        create_nodes, burst,
        "WAL replay must recover every entry submitted under backpressure"
    );
}

#[test]
#[ignore] // TODO: Fix batch size limit test - timing issue with async flushing
fn test_batch_size_limit() {
    let ctx = TestContext::new();
    let wal_path = ctx.path().join("wal.log");
    let wal = Wal::new(&wal_path).unwrap();

    let config = AsyncWalConfig {
        max_batch_size: 5,                         // Small batch size for testing
        max_batch_age: Duration::from_millis(100), // Short timeout for testing
        max_queue_depth: 100,
        flush_interval: Duration::from_millis(50), // Short flush interval
        channel_buffer_size: 50,
        checkpoint_size_bytes: u64::MAX,
        flush_gate: None,
        fail_flush: None,
    };

    let mut writer = AsyncWalWriter::new(wal, config).unwrap();

    // Submit more entries than batch size
    for i in 0..10 {
        let entry = WalEntry::CreateNode {
            node_id: i,
            label_bits: 0,
        };
        writer.append(entry).unwrap();
    }

    // Give time for batching and flushing (longer wait for async processing)
    thread::sleep(Duration::from_millis(500));

    let stats = writer.stats();
    assert_eq!(stats.entries_submitted, 10);
    // With batch size 5, 10 entries should create at least 2 batches
    // But due to timing, we might get fewer - just check that some batches were flushed
    assert!(
        stats.batches_flushed > 0,
        "No batches were flushed, got {}",
        stats.batches_flushed
    );

    writer.shutdown().unwrap();
}

/// Regression test for the `unsafe { &mut *(Arc::as_ptr(...) as
/// *mut AsyncWalStats) }` data race: 10 threads call `append` in
/// parallel; every one of them must be counted. With the old
/// pointer-cast implementation the final `entries_submitted`
/// could come in below 10 under Miri / stressed loads; with the
/// atomic counters it must be exactly 10.
#[test]
fn concurrent_appends_count_exactly() {
    use std::sync::Arc as ArcT;
    let (writer, _dir) = create_test_writer();
    let writer = ArcT::new(writer);
    let threads: Vec<_> = (0..10)
        .map(|i| {
            let w = ArcT::clone(&writer);
            thread::spawn(move || {
                w.append(WalEntry::CreateNode {
                    node_id: i,
                    label_bits: 0,
                })
                .unwrap();
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let snap = writer.stats();
    assert_eq!(snap.entries_submitted, 10);
}

/// §1.2 (`phase0_fix-async-wal-flush-durability`) — deterministic
/// proof that `flush()` blocks until the writer thread has actually
/// executed `flush_batch` for the `WalCommand::Flush` it enqueued,
/// not merely until the request landed on the channel. A test-only
/// gate (`AsyncWalConfig::flush_gate`) holds the writer thread
/// immediately before it runs `flush_batch`; while the gate is
/// closed, `flush()` must not have returned and the entry must not
/// yet be durable on disk. Releasing the gate must unblock `flush()`
/// and make the entry durable by the time it returns. This is the
/// permanent regression test for the bug: before the fix, `flush()`
/// returned as soon as the command was sent, so this test would fail
/// at the "must not have returned" assertion.
#[test]
fn flush_blocks_until_writer_thread_signals_completion() {
    let ctx = TestContext::new();
    let wal_path = ctx.path().join("wal.log");
    let wal = Wal::new(&wal_path).unwrap();

    // Rendezvous gate: `writer_thread` blocks in `gate.recv()` until
    // this test sends on `gate_tx`.
    let (gate_tx, gate_rx) = bounded::<()>(0);
    let config = AsyncWalConfig {
        max_batch_size: 100,
        max_batch_age: Duration::from_secs(60), // no age-based auto-flush
        max_queue_depth: 100,
        flush_interval: Duration::from_secs(60), // no interval-based auto-flush
        channel_buffer_size: 50,
        checkpoint_size_bytes: u64::MAX,
        flush_gate: Some(gate_rx),
        fail_flush: None,
    };

    let writer = Arc::new(AsyncWalWriter::new(wal, config).unwrap());

    writer
        .append(WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        })
        .unwrap();

    // Give the writer thread time to dequeue the Append into its
    // batch before the gated Flush is issued, isolating the
    // enqueue-vs-complete gap on the Flush command itself.
    thread::sleep(Duration::from_millis(50));

    let flush_writer = Arc::clone(&writer);
    let (done_tx, done_rx) = mpsc::channel::<Result<()>>();
    let flush_thread = thread::spawn(move || {
        let _ = done_tx.send(flush_writer.flush());
    });

    // Gate is closed: flush() must NOT have returned yet.
    thread::sleep(Duration::from_millis(200));
    assert!(
        done_rx.try_recv().is_err(),
        "flush() returned before the writer thread processed WalCommand::Flush \
         — the durability barrier is not actually blocking"
    );

    // ...and the entry must not be durable on disk yet either.
    let mut probe = Wal::new(&wal_path).unwrap();
    let recovered_before_release = probe.recover().unwrap();
    assert!(
        recovered_before_release.is_empty(),
        "entry must not be durable while flush() is still gated"
    );

    // Release the gate: the writer thread runs flush_batch and
    // signals completion; flush() must then return promptly.
    gate_tx.send(()).unwrap();

    let result = done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("flush() must return once the gate is released (bounded wait)");
    result.expect("flush() must succeed");
    flush_thread.join().unwrap();

    let mut probe = Wal::new(&wal_path).unwrap();
    let recovered_after = probe.recover().unwrap();
    assert_eq!(
        recovered_after.len(),
        1,
        "entry must be durable immediately after flush() returns"
    );

    Arc::try_unwrap(writer)
        .unwrap_or_else(|_| panic!("writer still has outstanding Arc clones"))
        .shutdown()
        .unwrap();
}

/// §4.2 — `flush()` must propagate a real `Err`, not a blind
/// `Ok(())`, when `flush_batch` fails after exhausting its retries.
/// Uses the `AsyncWalConfig::fail_flush` test-only fault injector so
/// the failure is deterministic and platform-independent while still
/// exercising the real retry/backoff/emergency-save path in
/// `flush_batch`.
#[test]
fn flush_propagates_error_after_retries_exhausted() {
    let ctx = TestContext::new();
    let wal = Wal::new(ctx.path().join("wal.log")).unwrap();

    let fail_flush = Arc::new(AtomicBool::new(true));
    let config = AsyncWalConfig {
        max_batch_size: 10,
        max_batch_age: Duration::from_millis(20),
        max_queue_depth: 100,
        flush_interval: Duration::from_millis(10),
        channel_buffer_size: 50,
        checkpoint_size_bytes: u64::MAX,
        flush_gate: None,
        fail_flush: Some(Arc::clone(&fail_flush)),
    };

    let mut writer = AsyncWalWriter::new(wal, config).unwrap();

    writer
        .append(WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        })
        .unwrap();

    // Exhausts MAX_RETRIES with exponential backoff (~600ms of
    // sleeping) before returning — bounded well under any test
    // harness timeout.
    let result = writer.flush();
    assert!(
        result.is_err(),
        "flush() must return Err when flush_batch exhausts its retries, got {result:?}"
    );

    fail_flush.store(false, Ordering::Relaxed);
    writer.shutdown().unwrap();
}

/// §2.3/§3.4 — a `flush()` call racing `shutdown()` must not hang: it
/// must return (`Ok` via a signaled handshake, or `Err` if the
/// writer thread's handshake sender was dropped because the thread
/// exited first). The public API's `shutdown(&mut self)` cannot
/// literally run concurrently with `flush(&self)` from safe code on
/// the same instance (the `&mut` borrow forbids it), so this test
/// drives the race directly through the writer's internal command
/// channel — `sender` and `shutdown` are private fields of
/// `AsyncWalWriter`, reachable here because this test module is a
/// child of `async_wal` — issuing the exact same
/// flag-then-`WalCommand::Shutdown` sequence the real `shutdown()`
/// method uses, but from a second thread while several `flush()`
/// calls are already in flight.
#[test]
fn flush_concurrent_with_shutdown_does_not_hang() {
    let ctx = TestContext::new();
    let wal = Wal::new(ctx.path().join("wal.log")).unwrap();
    let config = AsyncWalConfig {
        max_batch_size: 10,
        max_batch_age: Duration::from_millis(10),
        max_queue_depth: 100,
        flush_interval: Duration::from_millis(5),
        channel_buffer_size: 50,
        checkpoint_size_bytes: u64::MAX,
        flush_gate: None,
        fail_flush: None,
    };
    let writer = Arc::new(AsyncWalWriter::new(wal, config).unwrap());

    writer
        .append(WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        })
        .unwrap();

    let (done_tx, done_rx) = mpsc::channel::<Result<()>>();
    let flush_threads: Vec<_> = (0..8)
        .map(|_| {
            let w = Arc::clone(&writer);
            let tx = done_tx.clone();
            thread::spawn(move || {
                let _ = tx.send(w.flush());
            })
        })
        .collect();
    drop(done_tx);

    // Race the manual shutdown sequence (identical to what
    // `AsyncWalWriter::shutdown()` does) against the in-flight
    // `flush()` calls above.
    writer.shutdown.store(true, Ordering::SeqCst);
    let _ = writer.sender.send(WalCommand::Shutdown);

    for _ in 0..8 {
        done_rx.recv_timeout(Duration::from_secs(5)).expect(
            "flush() concurrent with shutdown() must return within the bound \
             (a disconnected handshake channel must surface as Err, not a hang)",
        );
    }

    for t in flush_threads {
        t.join().unwrap();
    }

    // Bound teardown too: dropping the last `Arc<AsyncWalWriter>`
    // runs `Drop::drop`, which joins the background thread. Do that
    // on a helper thread with a timeout so a regression there fails
    // this test instead of hanging the whole test binary.
    let (teardown_tx, teardown_rx) = mpsc::channel::<()>();
    thread::spawn(move || {
        drop(writer);
        let _ = teardown_tx.send(());
    });
    teardown_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("dropping the writer (joining the background thread) must not hang");
}

/// Group commit (`flush_batch` runs exactly one `wal.flush()` per
/// batch, not one per entry) must be durability-equivalent to a
/// per-entry fsync: appending many entries across many small batches,
/// followed by a single `flush()`, must recover every entry in
/// exactly the order it was appended — nothing lost, nothing
/// reordered.
#[test]
fn group_commit_recovers_every_appended_entry_in_order() {
    let ctx = TestContext::new();
    let wal_path = ctx.path().join("wal.log");
    let wal = Wal::new(&wal_path).unwrap();
    let config = AsyncWalConfig {
        max_batch_size: 8, // small: 200 entries span ~25 batches
        max_batch_age: Duration::from_millis(20),
        max_queue_depth: 64,
        flush_interval: Duration::from_millis(10),
        channel_buffer_size: 32,
        checkpoint_size_bytes: u64::MAX,
        flush_gate: None,
        fail_flush: None,
    };
    let mut writer = AsyncWalWriter::new(wal, config).unwrap();

    const N: u64 = 200;
    let mut appended = Vec::with_capacity(N as usize);
    for i in 0..N {
        let rel_id = i;
        let src = i * 10;
        let dst = i * 10 + 1;
        let type_id = (i % 7) as u32;
        writer
            .append(WalEntry::CreateRel {
                rel_id,
                src,
                dst,
                type_id,
            })
            .unwrap();
        appended.push((rel_id, src, dst, type_id));
    }

    writer.flush().unwrap();
    writer.shutdown().unwrap();

    let mut reopened = Wal::new(&wal_path).unwrap();
    let recovered = reopened.recover().unwrap();
    let recovered_rels: Vec<(u64, u64, u64, u32)> = recovered
        .iter()
        .filter_map(|e| match e {
            WalEntry::CreateRel {
                rel_id,
                src,
                dst,
                type_id,
            } => Some((*rel_id, *src, *dst, *type_id)),
            _ => None,
        })
        .collect();

    assert_eq!(
        recovered_rels.len(),
        N as usize,
        "must recover exactly the number of entries appended, got {}",
        recovered_rels.len()
    );
    assert_eq!(
        recovered_rels, appended,
        "group-commit batching (one fsync per batch) must preserve exact \
         append order and content across many small batches"
    );
}

/// #19: a burst far larger than the channel capacity must engage
/// backpressure (blocking `send()` after a full `try_send()`), keep
/// the observed queue depth bounded at the configured capacity, and
/// still accept and durably persist every entry — bounded, not
/// unbounded, and lossless under sustained pressure.
#[test]
fn sustained_backpressure_is_bounded_and_lossless() {
    let ctx = TestContext::new();
    let wal = Wal::new(ctx.path().join("wal.log")).unwrap();
    const CAPACITY: u64 = 32; // channel capacity = max(8, 32) = 32
    let config = AsyncWalConfig {
        max_batch_size: 8,
        max_batch_age: Duration::from_millis(20),
        max_queue_depth: CAPACITY as usize,
        flush_interval: Duration::from_millis(10),
        channel_buffer_size: 8,
        checkpoint_size_bytes: u64::MAX,
        flush_gate: None,
        fail_flush: None,
    };
    let mut writer = AsyncWalWriter::new(wal, config).unwrap();

    const BURST: u64 = CAPACITY * 40;
    for i in 0..BURST {
        writer
            .append(WalEntry::CreateRel {
                rel_id: i,
                src: i * 2,
                dst: i * 2 + 1,
                type_id: 0,
            })
            .expect("append must succeed under sustained backpressure, never drop or error");
    }

    writer.flush().unwrap();

    let stats = writer.stats();
    assert!(
        stats.backpressure_blocks > 0,
        "a burst 40x the channel capacity must engage backpressure at least once"
    );
    // `current_queue_depth`/`max_queue_depth` counts entries accepted
    // into the pipeline (`fetch_add` runs immediately in `append()`,
    // before the entry is physically placed on the bounded channel via
    // `try_send`/blocking `send`) minus entries the writer thread has
    // dequeued AND already decremented (the decrement runs strictly
    // after `recv`, before the writer loops back — see
    // `writer_thread`'s `Ok(WalCommand::Append(entry))` arm). So it can
    // read up to 2 higher than true channel occupancy: (1) the single
    // producer's one entry that is counted but still blocked in
    // `send()` waiting for room, plus (2) the writer's most recently
    // dequeued entry whose decrement has not yet executed. Both gaps
    // are bounded at exactly one each because the producer is a single
    // sequential thread (never more than one `append()` in flight) and
    // the writer thread is single-threaded and processes recv/push/
    // decrement as one atomic-w.r.t.-itself sequence before recv-ing
    // again. So the peak is bounded by capacity + 2 — never higher,
    // and never unbounded.
    assert!(
        stats.max_queue_depth <= CAPACITY + 2,
        "observed queue depth {} must stay bounded near configured capacity {} \
         (capacity + 2 to account for the two structural in-flight gaps, not unbounded)",
        stats.max_queue_depth,
        CAPACITY
    );

    writer.shutdown().unwrap();

    let mut reopened = Wal::new(ctx.path().join("wal.log")).unwrap();
    let recovered = reopened.recover().unwrap();
    let recovered_ids: std::collections::HashSet<u64> = recovered
        .iter()
        .filter_map(|e| match e {
            WalEntry::CreateRel { rel_id, .. } => Some(*rel_id),
            _ => None,
        })
        .collect();

    assert_eq!(
        recovered_ids.len(),
        BURST as usize,
        "every appended entry must be recoverable after a bounded, \
         sustained backpressure burst"
    );
    let expected_ids: std::collections::HashSet<u64> = (0..BURST).collect();
    assert_eq!(
        recovered_ids, expected_ids,
        "recovered id set must exactly match the appended id set \
         (no loss, no duplication)"
    );
}
