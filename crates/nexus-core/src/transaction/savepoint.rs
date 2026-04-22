//! Transaction savepoint stack (phase6_opencypher-advanced-types §5).
//!
//! A savepoint is a named marker inside an in-flight transaction that
//! the caller can later rewind to — the SQL semantics, lifted into
//! Cypher. Three operations:
//!
//! - `SAVEPOINT <name>` — push a marker; the marker captures the
//!   current undo-log and staged-op offsets.
//! - `ROLLBACK TO SAVEPOINT <name>` — pop every marker pushed after
//!   `<name>`, replay the undo log back to `<name>`'s offsets, truncate
//!   the staged-op journal, and keep `<name>` on the stack for future
//!   work.
//! - `RELEASE SAVEPOINT <name>` — pop `<name>` (and any inner markers)
//!   without undoing anything.
//!
//! Savepoints are purely in-memory. A committed transaction produces a
//! WAL entry indistinguishable from one without savepoints — the
//! caller has already folded its rollbacks into the final journal by
//! the time commit runs.
//!
//! This module is intentionally independent of the MVCC engine: it
//! exposes a generic stack over `UndoLog` + `StagedOps` offsets that
//! any future transaction rewrite can wire into with a single
//! `apply_until` call. Today the engine threads
//! [`SavepointStack::push`], [`SavepointStack::rollback_to`] and
//! [`SavepointStack::release`] through session state so the Cypher
//! surface behaves correctly even before the deep MVCC integration
//! lands.

use crate::{Error, Result};

/// Snapshot captured by [`SavepointStack::push`]. The runtime adapter
/// that owns the actual undo log decides what "offset" means — the
/// stack treats the number opaquely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavepointMarker {
    /// Offset into the tx's undo log when the savepoint was pushed.
    pub undo_log_offset: usize,
    /// Offset into the tx's staged-op journal when the savepoint was pushed.
    pub staged_ops_offset: usize,
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    marker: SavepointMarker,
}

/// Per-transaction savepoint stack. FIFO semantics with name lookup.
#[derive(Debug, Default, Clone)]
pub struct SavepointStack {
    stack: Vec<Entry>,
}

impl SavepointStack {
    /// Build an empty stack. Attached to every new write transaction
    /// and cleared on commit or rollback.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a new marker. Duplicate names are allowed — nested
    /// savepoints with the same name unwind LIFO, matching PostgreSQL.
    pub fn push(&mut self, name: &str, marker: SavepointMarker) {
        self.stack.push(Entry {
            name: name.to_string(),
            marker,
        });
    }

    /// Look up a marker by name (most-recent wins on duplicates) and
    /// pop every marker pushed *after* it. Returns the marker so the
    /// caller can replay the undo log.
    ///
    /// The named savepoint itself is left on the stack for further
    /// work, matching SQL's `ROLLBACK TO SAVEPOINT` semantics.
    pub fn rollback_to(&mut self, name: &str) -> Result<SavepointMarker> {
        for (i, entry) in self.stack.iter().enumerate().rev() {
            if entry.name == name {
                let marker = entry.marker;
                self.stack.truncate(i + 1);
                return Ok(marker);
            }
        }
        Err(Error::CypherExecution(format!(
            "ERR_SAVEPOINT_UNKNOWN: no savepoint named {name:?} on the stack"
        )))
    }

    /// Release a savepoint: pop the named marker and every marker
    /// pushed after it. No undo is performed.
    pub fn release(&mut self, name: &str) -> Result<()> {
        for (i, entry) in self.stack.iter().enumerate().rev() {
            if entry.name == name {
                self.stack.truncate(i);
                return Ok(());
            }
        }
        Err(Error::CypherExecution(format!(
            "ERR_SAVEPOINT_UNKNOWN: no savepoint named {name:?} on the stack"
        )))
    }

    /// Current depth of the stack (primarily for tests / instrumentation).
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Clear every marker. Called on commit/abort so the stack starts
    /// empty on the next transaction.
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    /// Names currently on the stack, outer-most first.
    pub fn names(&self) -> Vec<String> {
        self.stack.iter().map(|e| e.name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(u: usize, s: usize) -> SavepointMarker {
        SavepointMarker {
            undo_log_offset: u,
            staged_ops_offset: s,
        }
    }

    #[test]
    fn push_and_release_lifo() {
        let mut s = SavepointStack::new();
        s.push("s1", m(0, 0));
        s.push("s2", m(1, 1));
        s.release("s1").unwrap();
        assert_eq!(s.depth(), 0);
    }

    #[test]
    fn rollback_to_preserves_named_marker() {
        let mut s = SavepointStack::new();
        s.push("s1", m(0, 0));
        s.push("s2", m(1, 1));
        s.push("s3", m(2, 2));
        let marker = s.rollback_to("s2").unwrap();
        assert_eq!(marker, m(1, 1));
        // s2 stays on the stack; s3 was popped.
        assert_eq!(s.names(), vec!["s1".to_string(), "s2".to_string()]);
    }

    #[test]
    fn rollback_to_unknown_errors() {
        let mut s = SavepointStack::new();
        s.push("s1", m(0, 0));
        let err = s.rollback_to("ghost").unwrap_err();
        assert!(err.to_string().contains("ERR_SAVEPOINT_UNKNOWN"));
        // stack untouched on error
        assert_eq!(s.depth(), 1);
    }

    #[test]
    fn release_unknown_errors() {
        let mut s = SavepointStack::new();
        let err = s.release("ghost").unwrap_err();
        assert!(err.to_string().contains("ERR_SAVEPOINT_UNKNOWN"));
    }

    #[test]
    fn duplicate_names_unwind_lifo() {
        let mut s = SavepointStack::new();
        s.push("s", m(0, 0));
        s.push("s", m(5, 5));
        // rollback_to picks the most-recent entry first.
        let marker = s.rollback_to("s").unwrap();
        assert_eq!(marker, m(5, 5));
        // releasing now pops the remaining inner `s` (because it's
        // now the top).
        s.release("s").unwrap();
        assert_eq!(s.depth(), 1); // the outer `s` survives
    }

    #[test]
    fn nested_rollback_through_three_layers() {
        let mut s = SavepointStack::new();
        s.push("a", m(10, 100));
        s.push("b", m(20, 200));
        s.push("c", m(30, 300));
        let marker = s.rollback_to("a").unwrap();
        assert_eq!(marker, m(10, 100));
        assert_eq!(s.names(), vec!["a".to_string()]);
    }

    #[test]
    fn clear_resets_stack() {
        let mut s = SavepointStack::new();
        s.push("s1", m(0, 0));
        s.clear();
        assert_eq!(s.depth(), 0);
    }
}
