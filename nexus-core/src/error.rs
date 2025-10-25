//! Error types for Nexus Core

use thiserror::Error;

/// Result type alias using Nexus Error
pub type Result<T> = std::result::Result<T, Error>;

/// Core error types for Nexus graph database
#[derive(Error, Debug)]
pub enum Error {
    /// I/O errors from storage operations
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// LMDB/heed database errors
    #[error("Database error: {0}")]
    Database(#[from] heed::Error),

    /// Storage-related errors
    #[error("Storage error: {0}")]
    Storage(String),

    /// Page cache errors
    #[error("Page cache error: {0}")]
    PageCache(String),

    /// WAL (write-ahead log) errors
    #[error("WAL error: {0}")]
    Wal(String),

    /// Catalog errors (label/type/key mappings)
    #[error("Catalog error: {0}")]
    Catalog(String),

    /// Transaction errors
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Index errors
    #[error("Index error: {0}")]
    Index(String),

    /// Query executor errors
    #[error("Executor error: {0}")]
    Executor(String),

    /// Graph correlation analysis errors
    #[error("Graph correlation error: {0}")]
    GraphCorrelation(String),

    /// Retryable errors (temporary failures)
    #[error("Retryable error: {0}")]
    Retryable(String),

    /// Cypher parsing errors
    #[error("Cypher syntax error: {0}")]
    CypherSyntax(String),

    /// Invalid node/relationship ID
    #[error("Invalid ID: {0}")]
    InvalidId(String),

    /// Node or relationship not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Constraint violation (UNIQUE, NOT NULL, etc.)
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    /// Type mismatch errors
    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        /// Expected type
        expected: String,
        /// Actual type
        actual: String,
    },

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Create a storage error
    pub fn storage(msg: impl Into<String>) -> Self {
        Self::Storage(msg.into())
    }

    /// Create a page cache error
    pub fn page_cache(msg: impl Into<String>) -> Self {
        Self::PageCache(msg.into())
    }

    /// Create a WAL error
    pub fn wal(msg: impl Into<String>) -> Self {
        Self::Wal(msg.into())
    }

    /// Create a catalog error
    pub fn catalog(msg: impl Into<String>) -> Self {
        Self::Catalog(msg.into())
    }

    /// Create a transaction error
    pub fn transaction(msg: impl Into<String>) -> Self {
        Self::Transaction(msg.into())
    }

    /// Create an index error
    pub fn index(msg: impl Into<String>) -> Self {
        Self::Index(msg.into())
    }

    /// Create an executor error
    pub fn executor(msg: impl Into<String>) -> Self {
        Self::Executor(msg.into())
    }

    /// Create a graph correlation error
    pub fn graph_correlation(msg: impl Into<String>) -> Self {
        Self::GraphCorrelation(msg.into())
    }

    /// Create an internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}
