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
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Tantivy full-text search errors
    #[error("Tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    /// Query parser errors
    #[error("Query parser error: {0}")]
    QueryParser(#[from] tantivy::query::QueryParserError),

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

    /// Index consistency errors
    #[error("Index consistency error: {0}")]
    IndexConsistency(String),

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

    /// Cypher execution errors
    #[error("Cypher execution error: {0}")]
    CypherExecution(String),

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

    /// Deadlock detected
    #[error("Deadlock detected: {0}")]
    DeadlockDetected(String),

    /// Plugin errors
    #[error("Plugin error: {0}")]
    Plugin(String),

    /// Format/display error
    #[error("Format error: {0}")]
    Format(#[from] std::fmt::Error),

    /// Lock timeout
    #[error("Lock timeout: {0}")]
    LockTimeout(String),

    /// Out of memory
    #[error("Out of memory: {0}")]
    OutOfMemory(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Regex compilation errors
    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    /// Replication errors
    #[error("Replication error: {0}")]
    Replication(String),

    /// Cluster-mode quota exceeded — surfaced when a tenant has
    /// hit its per-namespace storage limit (or, in the future, a
    /// per-namespace rate limit that the engine layer gates on).
    /// Produced by `Engine::execute_cypher_with_context` when the
    /// installed [`crate::cluster::QuotaProvider`] denies a write.
    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),

    /// An external id already maps to a different node.
    ///
    /// Returned when `ConflictPolicy::Error` is active and the supplied
    /// external id is already present in the catalog index.
    #[error(
        "external id conflict: {attempted_external_id} already maps to internal id \
         {existing_internal_id}"
    )]
    ExternalIdConflict {
        /// The internal id that already holds the external id.
        existing_internal_id: u64,
        /// Display form of the external id that the caller attempted to assign.
        attempted_external_id: String,
    },
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

    /// Create a deadlock detected error
    pub fn deadlock_detected(msg: impl Into<String>) -> Self {
        Self::DeadlockDetected(msg.into())
    }

    /// Create a lock timeout error
    pub fn lock_timeout(msg: impl Into<String>) -> Self {
        Self::LockTimeout(msg.into())
    }

    /// Create an out of memory error
    pub fn out_of_memory(msg: impl Into<String>) -> Self {
        Self::OutOfMemory(msg.into())
    }

    /// Create an invalid input error
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    /// Create a replication error
    pub fn replication(msg: impl Into<String>) -> Self {
        Self::Replication(msg.into())
    }

    /// openCypher-TCK classification of this error. A structured `ERR_*`
    /// code prefix wins; otherwise the Rust variant decides. Errors we
    /// cannot confidently classify return [`OpenCypherErrorKind::Uncategorized`].
    pub fn opencypher_kind(&self) -> OpenCypherErrorKind {
        if let Some(kind) = self
            .structured_code()
            .and_then(OpenCypherErrorKind::from_error_code)
        {
            return kind;
        }
        match self {
            Error::CypherSyntax(_) | Error::QueryParser(_) => OpenCypherErrorKind::SyntaxError,
            Error::TypeMismatch { .. } => OpenCypherErrorKind::TypeError,
            Error::ConstraintViolation(_) => OpenCypherErrorKind::ConstraintVerificationFailed,
            Error::NotFound(_) | Error::InvalidId(_) => OpenCypherErrorKind::EntityNotFound,
            Error::InvalidInput(_) => OpenCypherErrorKind::ArgumentError,
            _ => OpenCypherErrorKind::Uncategorized,
        }
    }

    /// The leading `ERR_*` code token of the message, if present. Nexus
    /// emits these as a structured prefix (`"ERR_CRS_MISMATCH: …"`) on the
    /// string-bearing variants.
    fn structured_code(&self) -> Option<&str> {
        let msg = match self {
            Error::CypherSyntax(m)
            | Error::CypherExecution(m)
            | Error::Executor(m)
            | Error::InvalidInput(m)
            | Error::ConstraintViolation(m)
            | Error::Storage(m)
            | Error::Index(m)
            | Error::Catalog(m)
            | Error::Transaction(m)
            | Error::Internal(m)
            | Error::NotFound(m)
            | Error::InvalidId(m) => m.as_str(),
            _ => return None,
        };
        let msg = msg.trim_start();
        if !msg.starts_with("ERR_") {
            return None;
        }
        let end = msg
            .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(msg.len());
        Some(&msg[..end])
    }
}

/// openCypher-TCK error classification. Nexus's [`Error`] variants are
/// mapped onto these so the TCK harness can assert the *kind* of failure,
/// not merely that some substring appears in the message.
///
/// Precedence is code-first: a structured `ERR_*` prefix carries more
/// specific semantics than the coarse Rust variant and wins over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenCypherErrorKind {
    /// Malformed query text / static composition error.
    SyntaxError,
    /// Statically-detectable semantic error (scope, aggregation, union shape).
    SemanticError,
    /// Value/CRS/argument type mismatch.
    TypeError,
    /// Bad argument value, count, or shape.
    ArgumentError,
    /// A referenced node/relationship no longer exists.
    EntityNotFound,
    /// A constraint (uniqueness, index-build precondition, …) was violated.
    ConstraintVerificationFailed,
    /// A referenced query parameter was not supplied.
    ParameterMissing,
    /// A called procedure failed or does not exist.
    ProcedureError,
    /// Nexus cannot (yet) prove a specific openCypher kind for this error.
    Uncategorized,
}

impl OpenCypherErrorKind {
    /// Map a Nexus structured `ERR_*` code to a TCK kind. Codes absent here
    /// are either Cypher-surface-irrelevant (storage/crypto/WAL/sharding
    /// internals) or not yet validated against a TCK scenario, and fall
    /// back to variant-based classification. Extend as the full-corpus
    /// runner validates more codes against real scenarios.
    fn from_error_code(code: &str) -> Option<Self> {
        Some(match code {
            "ERR_CRS_MISMATCH" | "ERR_INVALID_ARG_TYPE" => Self::TypeError,
            "ERR_BBOX_MALFORMED" | "ERR_INVALID_ARG_VALUE" | "ERR_MISSING_ARG" => {
                Self::ArgumentError
            }
            "ERR_MISSING_PARAMETER" => Self::ParameterMissing,
            "ERR_PROC_NOT_FOUND" => Self::ProcedureError,
            "ERR_RTREE_BUILD" | "ERR_CONSTRAINT_VIOLATED" => Self::ConstraintVerificationFailed,
            _ => return None,
        })
    }

    /// Parse a TCK Gherkin error-kind word into a kind. Accepts the
    /// Nexus-spatial-corpus alias `ConstraintError` for the upstream
    /// `ConstraintVerificationFailed`.
    pub fn parse_tck_name(name: &str) -> Option<Self> {
        Some(match name {
            "SyntaxError" => Self::SyntaxError,
            "SemanticError" => Self::SemanticError,
            "TypeError" => Self::TypeError,
            "ArgumentError" => Self::ArgumentError,
            "EntityNotFound" => Self::EntityNotFound,
            "ConstraintVerificationFailed" | "ConstraintError" => {
                Self::ConstraintVerificationFailed
            }
            "ParameterMissing" => Self::ParameterMissing,
            "ProcedureError" => Self::ProcedureError,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crs_mismatch_code_overrides_syntax_variant_default() {
        let err = Error::CypherSyntax("ERR_CRS_MISMATCH: a=cartesian, b=wgs-84".into());
        assert_eq!(err.opencypher_kind(), OpenCypherErrorKind::TypeError);
    }

    #[test]
    fn crs_mismatch_code_classifies_from_execution_variant() {
        let err = Error::CypherExecution("ERR_CRS_MISMATCH: a=cartesian, b=wgs-84".into());
        assert_eq!(err.opencypher_kind(), OpenCypherErrorKind::TypeError);
    }

    #[test]
    fn rtree_build_code_classifies_as_constraint_verification_failed() {
        let err = Error::CypherExecution("ERR_RTREE_BUILD: node 5 has no bbox".into());
        assert_eq!(
            err.opencypher_kind(),
            OpenCypherErrorKind::ConstraintVerificationFailed
        );
    }

    #[test]
    fn uncoded_syntax_error_falls_back_to_variant_default() {
        let err = Error::CypherSyntax("Cypher syntax error: unexpected token".into());
        assert_eq!(err.opencypher_kind(), OpenCypherErrorKind::SyntaxError);
    }

    #[test]
    fn type_mismatch_variant_classifies_as_type_error() {
        let err = Error::TypeMismatch {
            expected: "Integer".into(),
            actual: "String".into(),
        };
        assert_eq!(err.opencypher_kind(), OpenCypherErrorKind::TypeError);
    }

    #[test]
    fn constraint_violation_variant_classifies_as_constraint_verification_failed() {
        let err = Error::ConstraintViolation("unique".into());
        assert_eq!(
            err.opencypher_kind(),
            OpenCypherErrorKind::ConstraintVerificationFailed
        );
    }

    #[test]
    fn not_found_variant_classifies_as_entity_not_found() {
        let err = Error::NotFound("node 9".into());
        assert_eq!(err.opencypher_kind(), OpenCypherErrorKind::EntityNotFound);
    }

    #[test]
    fn uncoded_execution_error_falls_back_to_uncategorized() {
        let err = Error::CypherExecution("some uncoded runtime failure".into());
        assert_eq!(err.opencypher_kind(), OpenCypherErrorKind::Uncategorized);
    }

    #[test]
    fn missing_parameter_code_classifies_from_execution_variant() {
        let err = Error::CypherExecution("ERR_MISSING_PARAMETER: $foo".into());
        assert_eq!(err.opencypher_kind(), OpenCypherErrorKind::ParameterMissing);
    }

    #[test]
    fn parse_tck_name_accepts_constraint_error_alias_and_rejects_unknown() {
        assert_eq!(
            OpenCypherErrorKind::parse_tck_name("ConstraintError"),
            Some(OpenCypherErrorKind::ConstraintVerificationFailed)
        );
        assert_eq!(
            OpenCypherErrorKind::parse_tck_name("TypeError"),
            Some(OpenCypherErrorKind::TypeError)
        );
        assert!(OpenCypherErrorKind::parse_tck_name("Nonsense").is_none());
    }

    #[test]
    fn trailing_code_is_not_treated_as_a_leading_structured_code() {
        let err = Error::CypherExecution("failed: ERR_CRS_MISMATCH".into());
        assert_eq!(err.opencypher_kind(), OpenCypherErrorKind::Uncategorized);
    }
}
