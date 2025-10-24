//! Cypher executor - Pattern matching, expand, filter, project
//!
//! Physical operators:
//! - NodeByLabel(label) → scan bitmap
//! - FilterProps(predicate) → apply in batch
//! - Expand(type, direction) → use linked lists (next_src_ptr/next_dst_ptr)
//! - Project, Aggregate, Order, Limit
//!
//! Heuristic cost-based planning:
//! - Statistics per label (|V|), per type (|E|), average degree
//! - Reorder patterns for selectivity

use crate::{Error, Result};

/// Cypher query
#[derive(Debug, Clone)]
pub struct Query {
    /// Query string
    pub cypher: String,
}

/// Query result row
#[derive(Debug, Clone)]
pub struct Row {
    /// Column values
    pub values: Vec<serde_json::Value>,
}

/// Query result set
#[derive(Debug, Clone)]
pub struct ResultSet {
    /// Column names
    pub columns: Vec<String>,
    /// Result rows
    pub rows: Vec<Row>,
}

/// Physical operator
#[derive(Debug, Clone)]
pub enum Operator {
    /// Scan nodes by label
    NodeByLabel {
        /// Label ID
        label_id: u32,
    },
    /// Filter by property predicate
    Filter {
        /// Predicate expression
        predicate: String,
    },
    /// Expand relationships
    Expand {
        /// Type ID (None = all types)
        type_id: Option<u32>,
        /// Direction (Outgoing, Incoming, Both)
        direction: Direction,
    },
    /// Project columns
    Project {
        /// Column expressions
        columns: Vec<String>,
    },
    /// Limit results
    Limit {
        /// Maximum rows
        count: usize,
    },
}

/// Relationship direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Outgoing edges
    Outgoing,
    /// Incoming edges
    Incoming,
    /// Both directions
    Both,
}

/// Query executor
pub struct Executor {
    // Will parse Cypher, build plan, execute operators
}

impl Executor {
    /// Create a new executor
    pub fn new() -> Result<Self> {
        todo!("Executor::new - to be implemented in MVP")
    }

    /// Execute a Cypher query
    pub fn execute(&mut self, _query: &Query) -> Result<ResultSet> {
        todo!("execute - to be implemented in MVP")
    }

    /// Parse Cypher into physical plan
    pub fn parse_and_plan(&self, _cypher: &str) -> Result<Vec<Operator>> {
        todo!("parse_and_plan - to be implemented in MVP")
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new().expect("Failed to create default executor")
    }
}
