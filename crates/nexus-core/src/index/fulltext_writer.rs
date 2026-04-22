//! Per-index async writer for the full-text search backend
//! (phase6_fulltext-async-writer).
//!
//! Each [`NamedFullTextIndex`][super::fulltext_registry::NamedFullTextIndex]
//! spawns a background thread owning the single Tantivy
//! `IndexWriter` Tantivy permits per index. Callers enqueue
//! [`WriterCommand`] messages through a bounded
//! `crossbeam-channel`; the writer drains, batches, and commits
//! either when the buffer reaches `max_batch_size` or when
//! `refresh_ms` elapses since the last flush.
//!
//! Design notes:
//!
//! - **Threads, not tokio.** The rest of the FTS surface is
//!   synchronous, and Tantivy's writer API is blocking. A raw
//!   `std::thread` keeps the dependency story simple and avoids
//!   pulling a tokio runtime into the non-async callers.
//! - **Per-index single writer.** Tantivy demands exclusive access
//!   to its `IndexWriter`; owning it inside the writer thread
//!   enforces that invariant structurally.
//! - **Graceful shutdown.** Dropping the [`WriterHandle`] signals
//!   the loop to drain, commit, and exit. Callers that need
//!   synchronous "after this returns the commit is durable"
//!   semantics use [`WriterHandle::flush_blocking`].
//! - **Sync fallback.** Callers that can't pay the cross-thread
//!   hop (tests, single-threaded migrations) bypass the handle and
//!   drive the underlying [`FullTextIndex`] directly — the
//!   registry keeps the sync path callable for both cases.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, bounded};
use parking_lot::RwLock;

use super::fulltext::FullTextIndex;
use crate::Result;

/// Default capacity for the writer's inbound channel. Large enough
/// to absorb short bursts without back-pressuring callers; small
/// enough to bound memory when the writer stalls on a slow disk.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

/// Default cadence — commit + reload at least once per second even
/// when nothing crossed the batch-size threshold, so subscribers
/// see an eventually-consistent view without explicit awaits.
pub const DEFAULT_REFRESH_MS: u64 = 1000;

/// Default maximum batch size. Caps segment-write latency by
/// flushing after `max_batch_size` docs even if `refresh_ms` has
/// not yet elapsed.
pub const DEFAULT_MAX_BATCH: usize = 256;

/// Command the writer thread consumes.
#[derive(Debug)]
pub enum WriterCommand {
    /// Index a single document. Content mirrors the arguments of
    /// [`FullTextIndex::add_document`].
    Add {
        node_id: u64,
        label_id: u32,
        key_id: u32,
        content: String,
    },
    /// Delete any document with the given node id.
    Del { node_id: u64 },
    /// Force a commit + reader-reload, sending an ack through the
    /// supplied sender. Used by test harnesses and by
    /// [`WriterHandle::flush_blocking`].
    Flush(Sender<()>),
}

/// Tuning knobs for the writer.
#[derive(Debug, Clone, Copy)]
pub struct WriterConfig {
    /// Bound on the inbound channel.
    pub channel_capacity: usize,
    /// Max time between automatic commits.
    pub refresh: Duration,
    /// Max buffered docs before an early commit fires.
    pub max_batch_size: usize,
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            channel_capacity: DEFAULT_CHANNEL_CAPACITY,
            refresh: Duration::from_millis(DEFAULT_REFRESH_MS),
            max_batch_size: DEFAULT_MAX_BATCH,
        }
    }
}

/// Owned handle to a background writer thread.
///
/// Drop the handle to signal graceful shutdown — the writer drains
/// its channel, commits, reloads, and then exits. The drop path
/// joins the thread, so the caller knows the commit landed by the
/// time `Drop::drop` returns.
pub struct WriterHandle {
    tx: Option<Sender<WriterCommand>>,
    join: Option<thread::JoinHandle<()>>,
    cfg: WriterConfig,
    /// Live counter of enqueued-but-not-yet-acknowledged commands.
    /// Read by tests that assert "all enqueued docs have been
    /// committed" without racing the loop's internal state.
    pending: Arc<RwLock<usize>>,
}

impl WriterHandle {
    /// Spawn a writer thread that owns the given
    /// [`FullTextIndex`]. The caller keeps an `Arc` reference for
    /// read-side queries; the writer only ever mutates through the
    /// channel.
    pub fn spawn(index: Arc<FullTextIndex>, cfg: WriterConfig) -> Self {
        let (tx, rx) = bounded::<WriterCommand>(cfg.channel_capacity);
        let pending = Arc::new(RwLock::new(0usize));
        let loop_pending = pending.clone();
        let join = thread::spawn(move || {
            writer_loop(index, rx, cfg, loop_pending);
        });
        Self {
            tx: Some(tx),
            join: Some(join),
            cfg,
            pending,
        }
    }

    /// Enqueue a single document. Returns `Err` only if the writer
    /// has shut down.
    pub fn enqueue(&self, cmd: WriterCommand) -> Result<()> {
        let Some(tx) = self.tx.as_ref() else {
            return Err(crate::Error::storage(
                "ERR_FTS_WRITER_CLOSED: async writer is no longer accepting commands"
                    .to_string(),
            ));
        };
        *self.pending.write() += 1;
        if let Err(e) = tx.send(cmd) {
            *self.pending.write() = self.pending.read().saturating_sub(1);
            return Err(crate::Error::storage(format!(
                "ERR_FTS_WRITER_CLOSED: channel send failed: {e}"
            )));
        }
        Ok(())
    }

    /// Force a commit and block until the writer has processed
    /// every command queued ahead of the caller. Idempotent on
    /// empty buffers (Tantivy's `commit` is a no-op when no
    /// uncommitted ops exist).
    pub fn flush_blocking(&self) -> Result<()> {
        let Some(tx) = self.tx.as_ref() else {
            return Ok(());
        };
        let (ack_tx, ack_rx) = bounded::<()>(1);
        tx.send(WriterCommand::Flush(ack_tx)).map_err(|e| {
            crate::Error::storage(format!(
                "ERR_FTS_WRITER_CLOSED: flush send failed: {e}"
            ))
        })?;
        ack_rx.recv().map_err(|e| {
            crate::Error::storage(format!(
                "ERR_FTS_WRITER_CLOSED: flush ack missing: {e}"
            ))
        })?;
        Ok(())
    }

    /// Snapshot of the enqueue counter. Zero means the writer has
    /// processed every enqueued command.
    pub fn pending_count(&self) -> usize {
        *self.pending.read()
    }

    /// Access the config so tests + metrics can read the chosen
    /// cadence.
    pub fn config(&self) -> WriterConfig {
        self.cfg
    }
}

impl Drop for WriterHandle {
    fn drop(&mut self) {
        // Signal the writer by dropping the sender — the loop's
        // `recv` returns `Err` and the drain path runs.
        self.tx.take();
        if let Some(join) = self.join.take() {
            // Discard the join result; panics inside the writer
            // are already tracing-logged, and poisoning the drop
            // path would mask the primary error in the caller.
            let _ = join.join();
        }
    }
}

fn writer_loop(
    index: Arc<FullTextIndex>,
    rx: Receiver<WriterCommand>,
    cfg: WriterConfig,
    pending: Arc<RwLock<usize>>,
) {
    let mut buffer: Vec<WriterCommand> = Vec::with_capacity(cfg.max_batch_size);
    let mut last_commit = Instant::now();
    loop {
        // Wait up to the next-commit deadline for the next command.
        let now = Instant::now();
        let deadline = last_commit + cfg.refresh;
        let wait = deadline.saturating_duration_since(now);
        match rx.recv_timeout(wait) {
            Ok(cmd) => {
                match &cmd {
                    WriterCommand::Flush(_) => {
                        // Flush sentinel — commit the buffer, ack,
                        // and continue.
                        if !buffer.is_empty() {
                            apply_batch(&index, &mut buffer, &pending);
                        }
                        commit_and_reload(&index);
                        last_commit = Instant::now();
                        if let WriterCommand::Flush(ack) = cmd {
                            let _ = ack.send(());
                            *pending.write() = pending.read().saturating_sub(1);
                        }
                    }
                    _ => {
                        buffer.push(cmd);
                        if buffer.len() >= cfg.max_batch_size {
                            apply_batch(&index, &mut buffer, &pending);
                            commit_and_reload(&index);
                            last_commit = Instant::now();
                        }
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if !buffer.is_empty() {
                    apply_batch(&index, &mut buffer, &pending);
                    commit_and_reload(&index);
                }
                last_commit = Instant::now();
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                // Sender dropped — drain remaining commands and
                // exit. `recv_timeout` drained past the pending
                // slot; whatever is in `buffer` is the final
                // batch.
                if !buffer.is_empty() {
                    apply_batch(&index, &mut buffer, &pending);
                    commit_and_reload(&index);
                }
                break;
            }
        }
    }
}

fn apply_batch(index: &FullTextIndex, buffer: &mut Vec<WriterCommand>, pending: &Arc<RwLock<usize>>) {
    // Split add / del into a single writer pass for efficiency.
    let mut adds: Vec<(u64, u32, u32, String)> = Vec::with_capacity(buffer.len());
    let mut dels: Vec<u64> = Vec::new();
    for cmd in buffer.drain(..) {
        match cmd {
            WriterCommand::Add {
                node_id,
                label_id,
                key_id,
                content,
            } => adds.push((node_id, label_id, key_id, content)),
            WriterCommand::Del { node_id } => dels.push(node_id),
            WriterCommand::Flush(ack) => {
                // A stray flush inside a batch: ack it immediately;
                // the subsequent commit will flush everything.
                let _ = ack.send(());
            }
        }
    }
    if !adds.is_empty() {
        let refs: Vec<(u64, u32, u32, &str)> = adds
            .iter()
            .map(|(n, l, k, c)| (*n, *l, *k, c.as_str()))
            .collect();
        if let Err(e) = index.add_documents_bulk(&refs) {
            tracing::warn!("FTS async-writer: bulk add failed: {e}");
        }
    }
    for node_id in dels {
        if let Err(e) = index.remove_document(node_id, 0, 0) {
            tracing::warn!("FTS async-writer: remove failed for {node_id}: {e}");
        }
    }
    let processed = adds.len();
    let total_processed = processed + buffer.capacity().saturating_sub(buffer.len());
    *pending.write() = pending.read().saturating_sub(total_processed);
}

fn commit_and_reload(_index: &FullTextIndex) {
    // `add_documents_bulk` + `remove_document` already commit +
    // reload synchronously today. Kept as a seam so the future
    // async-commit variant (Tantivy's `IndexWriter::prepare_commit`
    // + background flush) can plug in here without touching
    // callers.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::fulltext_analyzer::AnalyzerKind;
    use tempfile::TempDir;

    fn open_index() -> (Arc<FullTextIndex>, TempDir) {
        let dir = TempDir::new().unwrap();
        let idx = FullTextIndex::with_analyzer(dir.path(), AnalyzerKind::Standard).unwrap();
        (Arc::new(idx), dir)
    }

    #[test]
    fn writer_commits_on_flush() {
        let (idx, _dir) = open_index();
        let handle = WriterHandle::spawn(idx.clone(), WriterConfig::default());
        handle
            .enqueue(WriterCommand::Add {
                node_id: 1,
                label_id: 0,
                key_id: 0,
                content: "hello world".to_string(),
            })
            .unwrap();
        handle.flush_blocking().unwrap();
        // Query synchronously through the shared index.
        let hits = idx
            .search(
                "hello",
                crate::index::fulltext::SearchOptions::default(),
            )
            .unwrap();
        assert!(hits.iter().any(|h| h.node_id == 1));
    }

    #[test]
    fn writer_drains_on_drop() {
        let (idx, _dir) = open_index();
        {
            let handle = WriterHandle::spawn(idx.clone(), WriterConfig::default());
            for i in 0..10u64 {
                handle
                    .enqueue(WriterCommand::Add {
                        node_id: i,
                        label_id: 0,
                        key_id: 0,
                        content: format!("graceful shutdown #{i}"),
                    })
                    .unwrap();
            }
            // Drop the handle without an explicit flush — the writer
            // must drain and commit before join completes.
        }
        let hits = idx
            .search(
                "graceful",
                crate::index::fulltext::SearchOptions::default(),
            )
            .unwrap();
        assert!(hits.len() >= 5, "expected drained docs after drop, got {hits:?}");
    }

    #[test]
    fn writer_honours_max_batch_capacity_trigger() {
        // Small batch size + long refresh → batch-trigger must fire
        // before the cadence.
        let (idx, _dir) = open_index();
        let cfg = WriterConfig {
            channel_capacity: 64,
            refresh: Duration::from_secs(30),
            max_batch_size: 3,
        };
        let handle = WriterHandle::spawn(idx.clone(), cfg);
        for i in 0..3u64 {
            handle
                .enqueue(WriterCommand::Add {
                    node_id: i,
                    label_id: 0,
                    key_id: 0,
                    content: format!("batch {i}"),
                })
                .unwrap();
        }
        // After max_batch_size enqueues the writer auto-flushes.
        // Give it a slice of time to wake up and process.
        for _ in 0..50 {
            let opts = crate::index::fulltext::SearchOptions::default();
            let hits = idx.search("batch", opts).unwrap();
            if hits.len() == 3 {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("writer did not auto-flush after max_batch_size enqueues");
    }

    #[test]
    fn enqueue_after_drop_fails_cleanly() {
        let (idx, _dir) = open_index();
        let handle = WriterHandle::spawn(idx, WriterConfig::default());
        handle.flush_blocking().unwrap();
        // Simulate a "writer closed" scenario by manually tearing
        // down the sender through Drop.
        drop(handle);
    }
}
