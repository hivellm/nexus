//! Raft log.
//!
//! In-memory Vec-backed append-only log with the invariants Raft needs:
//!
//! * Indices are 1-based (index 0 = "before the log").
//! * Entries are stored contiguous starting at [`RaftLog::first_index`].
//! * Truncating on term mismatch discards the tail (§5.3 of the Raft paper).
//! * Snapshots compact the log by advancing `first_index` past everything
//!   included in the snapshot.
//!
//! Persistence is intentionally out of scope for this module: the Raft
//! spec only requires that the log + current term + voted-for survive
//! crashes, and the outer shard plugs in its own storage. The unit tests
//! exercise the pure in-memory behavior; the integration story is in the
//! harness tests (see [`super::cluster`]).

use serde::{Deserialize, Serialize};

use super::types::{LogIndex, Term};

/// A single log entry. `command` is opaque bincode bytes chosen by the
/// state machine that consumes the log (for Nexus shards this is a
/// Cypher write or a metadata change).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Term in which this entry was created.
    pub term: Term,
    /// Position within the log (1-based).
    pub index: LogIndex,
    /// Opaque payload. State machine decides how to decode.
    pub command: Vec<u8>,
}

/// In-memory Raft log.
#[derive(Debug, Clone)]
pub struct RaftLog {
    /// Entries in monotonically increasing order by `index`.
    entries: Vec<LogEntry>,
    /// Index of the last entry compacted into the snapshot. Entries
    /// stored start at `snapshot_last_index + 1`. Zero when no
    /// snapshot has been installed.
    snapshot_last_index: LogIndex,
    /// Term of the entry at `snapshot_last_index`. Zero when no
    /// snapshot has been installed.
    snapshot_last_term: Term,
}

impl Default for RaftLog {
    fn default() -> Self {
        Self::new()
    }
}

impl RaftLog {
    /// Fresh empty log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            snapshot_last_index: LogIndex::ZERO,
            snapshot_last_term: Term(0),
        }
    }

    /// First valid index: the entry right after the snapshot. An empty
    /// log starts at 1.
    #[inline]
    #[must_use]
    pub fn first_index(&self) -> LogIndex {
        LogIndex(self.snapshot_last_index.0 + 1)
    }

    /// Last valid index, inclusive. Zero on an empty log with no
    /// snapshot.
    #[inline]
    #[must_use]
    pub fn last_index(&self) -> LogIndex {
        self.entries
            .last()
            .map(|e| e.index)
            .unwrap_or(self.snapshot_last_index)
    }

    /// Term of the entry at `last_index()`.
    #[must_use]
    pub fn last_term(&self) -> Term {
        self.entries
            .last()
            .map(|e| e.term)
            .unwrap_or(self.snapshot_last_term)
    }

    /// Term of the entry at `index`, or `None` if `index` is out of
    /// range or has been compacted into the snapshot (other than the
    /// snapshot's last entry itself).
    #[must_use]
    pub fn term_at(&self, index: LogIndex) -> Option<Term> {
        if index == LogIndex::ZERO {
            return Some(Term(0));
        }
        if index == self.snapshot_last_index {
            return Some(self.snapshot_last_term);
        }
        self.entry_at(index).map(|e| e.term)
    }

    /// Immutable view of the entry at `index`, if present.
    #[must_use]
    pub fn entry_at(&self, index: LogIndex) -> Option<&LogEntry> {
        if index < self.first_index() {
            return None;
        }
        let offset = (index.0 - self.first_index().0) as usize;
        self.entries.get(offset)
    }

    /// Entries starting at `from` (inclusive) up to `count` entries. If
    /// `from` is below the snapshot cutoff, returns an empty slice (the
    /// caller must issue InstallSnapshot instead).
    #[must_use]
    pub fn entries_from(&self, from: LogIndex, count: usize) -> &[LogEntry] {
        if from < self.first_index() {
            return &[];
        }
        let offset = (from.0 - self.first_index().0) as usize;
        let end = (offset + count).min(self.entries.len());
        if offset >= self.entries.len() {
            return &[];
        }
        &self.entries[offset..end]
    }

    /// Append a fresh entry. Used by leaders when accepting a client
    /// write. The caller is responsible for bumping term; this method
    /// just allocates the next index.
    pub fn append_command(&mut self, term: Term, command: Vec<u8>) -> LogIndex {
        let index = self.last_index().next();
        self.entries.push(LogEntry {
            term,
            index,
            command,
        });
        index
    }

    /// Follower receives entries from the leader. Applies Raft §5.3:
    /// if an existing entry conflicts with one from the leader
    /// (same index, different term), delete that entry and everything
    /// after it.
    ///
    /// Returns the index of the last accepted entry.
    pub fn append_follower(&mut self, entries: Vec<LogEntry>) -> LogIndex {
        for e in entries {
            if e.index <= self.last_index() {
                if let Some(existing) = self.entry_at(e.index) {
                    if existing.term == e.term {
                        // Same entry, skip.
                        continue;
                    }
                    // Conflict — truncate everything from `e.index` on.
                    self.truncate_from(e.index);
                }
            }
            // After possible truncation, just push.
            // Handle the case where `e.index > last_index() + 1`: the
            // leader is sending a non-contiguous entry, which must not
            // happen under correct Raft semantics. We still reject
            // silently rather than panic by only pushing when the index
            // lines up.
            if e.index == self.last_index().next() {
                self.entries.push(e);
            }
        }
        self.last_index()
    }

    /// Drop all entries with `index >= from`. Used on term conflicts.
    pub fn truncate_from(&mut self, from: LogIndex) {
        if from <= self.first_index() && from.0 > 0 {
            // from is at or below the snapshot boundary — nothing valid
            // to truncate besides already-compacted entries.
            self.entries.clear();
            return;
        }
        let first = self.first_index();
        if from.0 <= first.0 {
            self.entries.clear();
            return;
        }
        let keep = (from.0 - first.0) as usize;
        self.entries.truncate(keep);
    }

    /// Install a snapshot: discard every entry up through
    /// `last_included_index`, record the snapshot metadata.
    pub fn install_snapshot(&mut self, last_included_index: LogIndex, last_included_term: Term) {
        // Keep entries strictly after `last_included_index`.
        self.entries.retain(|e| e.index > last_included_index);
        self.snapshot_last_index = last_included_index;
        self.snapshot_last_term = last_included_term;
    }

    /// Snapshot metadata tuple.
    #[must_use]
    pub fn snapshot_meta(&self) -> (LogIndex, Term) {
        (self.snapshot_last_index, self.snapshot_last_term)
    }

    /// True when the log has no entries AND no snapshot.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.snapshot_last_index == LogIndex::ZERO
    }

    /// Count of entries actually held in memory (not counting the
    /// snapshot).
    #[must_use]
    pub fn live_len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(tag: u8) -> Vec<u8> {
        vec![tag]
    }

    #[test]
    fn empty_log_invariants() {
        let l = RaftLog::new();
        assert!(l.is_empty());
        assert_eq!(l.first_index(), LogIndex(1));
        assert_eq!(l.last_index(), LogIndex::ZERO);
        assert_eq!(l.last_term(), Term(0));
        assert!(l.entry_at(LogIndex(1)).is_none());
    }

    #[test]
    fn append_command_allocates_sequential_indices() {
        let mut l = RaftLog::new();
        assert_eq!(l.append_command(Term(1), cmd(1)), LogIndex(1));
        assert_eq!(l.append_command(Term(1), cmd(2)), LogIndex(2));
        assert_eq!(l.append_command(Term(2), cmd(3)), LogIndex(3));
        assert_eq!(l.last_index(), LogIndex(3));
        assert_eq!(l.last_term(), Term(2));
        assert_eq!(l.entry_at(LogIndex(2)).unwrap().term, Term(1));
    }

    #[test]
    fn term_at_zero_is_zero() {
        let l = RaftLog::new();
        assert_eq!(l.term_at(LogIndex::ZERO), Some(Term(0)));
    }

    #[test]
    fn term_at_present_entry() {
        let mut l = RaftLog::new();
        l.append_command(Term(3), cmd(1));
        assert_eq!(l.term_at(LogIndex(1)), Some(Term(3)));
    }

    #[test]
    fn term_at_missing_returns_none() {
        let l = RaftLog::new();
        assert_eq!(l.term_at(LogIndex(7)), None);
    }

    #[test]
    fn append_follower_contiguous() {
        let mut l = RaftLog::new();
        let entries = vec![
            LogEntry {
                term: Term(1),
                index: LogIndex(1),
                command: cmd(1),
            },
            LogEntry {
                term: Term(1),
                index: LogIndex(2),
                command: cmd(2),
            },
        ];
        assert_eq!(l.append_follower(entries), LogIndex(2));
        assert_eq!(l.last_index(), LogIndex(2));
    }

    #[test]
    fn append_follower_truncates_on_term_conflict() {
        let mut l = RaftLog::new();
        l.append_command(Term(1), cmd(1));
        l.append_command(Term(1), cmd(2));
        l.append_command(Term(1), cmd(3));

        // Leader from a new term overwrites index 2 onward with its own
        // version.
        let conflicting = vec![
            LogEntry {
                term: Term(2),
                index: LogIndex(2),
                command: cmd(20),
            },
            LogEntry {
                term: Term(2),
                index: LogIndex(3),
                command: cmd(30),
            },
        ];
        l.append_follower(conflicting);

        assert_eq!(l.last_index(), LogIndex(3));
        assert_eq!(l.entry_at(LogIndex(2)).unwrap().term, Term(2));
        assert_eq!(l.entry_at(LogIndex(2)).unwrap().command, cmd(20));
        assert_eq!(l.entry_at(LogIndex(3)).unwrap().term, Term(2));
    }

    #[test]
    fn append_follower_skips_matching_entries() {
        let mut l = RaftLog::new();
        l.append_command(Term(1), cmd(1));
        l.append_command(Term(1), cmd(2));

        // Leader resends matching entries; log stays the same length.
        let duplicate = vec![LogEntry {
            term: Term(1),
            index: LogIndex(2),
            command: cmd(2),
        }];
        l.append_follower(duplicate);
        assert_eq!(l.last_index(), LogIndex(2));
        assert_eq!(l.live_len(), 2);
    }

    #[test]
    fn truncate_from_discards_tail() {
        let mut l = RaftLog::new();
        for i in 1..=5 {
            l.append_command(Term(1), cmd(i as u8));
        }
        l.truncate_from(LogIndex(3));
        assert_eq!(l.last_index(), LogIndex(2));
        assert_eq!(l.live_len(), 2);
    }

    #[test]
    fn truncate_from_before_first_clears_all() {
        let mut l = RaftLog::new();
        l.append_command(Term(1), cmd(1));
        l.append_command(Term(1), cmd(2));
        l.truncate_from(LogIndex(1));
        assert!(l.is_empty());
    }

    #[test]
    fn entries_from_slice_respects_bounds() {
        let mut l = RaftLog::new();
        for _ in 0..5 {
            l.append_command(Term(1), cmd(0));
        }
        let slice = l.entries_from(LogIndex(2), 2);
        assert_eq!(slice.len(), 2);
        assert_eq!(slice[0].index, LogIndex(2));
        assert_eq!(slice[1].index, LogIndex(3));
    }

    #[test]
    fn entries_from_past_end_returns_empty() {
        let mut l = RaftLog::new();
        l.append_command(Term(1), cmd(0));
        assert!(l.entries_from(LogIndex(10), 3).is_empty());
    }

    #[test]
    fn install_snapshot_compacts() {
        let mut l = RaftLog::new();
        for _ in 0..5 {
            l.append_command(Term(1), cmd(0));
        }
        l.install_snapshot(LogIndex(3), Term(1));
        assert_eq!(l.snapshot_meta(), (LogIndex(3), Term(1)));
        assert_eq!(l.first_index(), LogIndex(4));
        assert_eq!(l.last_index(), LogIndex(5));
        assert!(l.entry_at(LogIndex(2)).is_none());
        assert!(l.entry_at(LogIndex(4)).is_some());
    }

    #[test]
    fn install_snapshot_past_tail_empties_log() {
        let mut l = RaftLog::new();
        l.append_command(Term(1), cmd(0));
        l.install_snapshot(LogIndex(10), Term(3));
        assert_eq!(l.first_index(), LogIndex(11));
        assert_eq!(l.last_index(), LogIndex(10));
        assert_eq!(l.last_term(), Term(3));
        assert_eq!(l.live_len(), 0);
    }

    #[test]
    fn term_at_matches_snapshot_boundary() {
        let mut l = RaftLog::new();
        l.install_snapshot(LogIndex(100), Term(4));
        assert_eq!(l.term_at(LogIndex(100)), Some(Term(4)));
        assert_eq!(l.term_at(LogIndex(99)), None);
    }

    #[test]
    fn log_entry_roundtrips_through_bincode() {
        let e = LogEntry {
            term: Term(7),
            index: LogIndex(42),
            command: b"hello".to_vec(),
        };
        let bytes = bincode::serialize(&e).unwrap();
        let back: LogEntry = bincode::deserialize(&bytes).unwrap();
        assert_eq!(e, back);
    }
}
