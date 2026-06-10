//! Tests for `Error` variant construction, formatting, and conversions.

use super::*;

#[test]
fn test_error_storage() {
    let err = Error::storage("test error");
    assert!(matches!(err, Error::Storage(_)));
    assert_eq!(err.to_string(), "Storage error: test error");
}

#[test]
fn test_error_page_cache() {
    let err = Error::page_cache("cache full");
    assert!(matches!(err, Error::PageCache(_)));
}

#[test]
fn test_error_wal() {
    let err = Error::wal("checkpoint failed");
    assert!(matches!(err, Error::Wal(_)));
}

#[test]
fn test_error_catalog() {
    let err = Error::catalog("catalog error");
    assert!(matches!(err, Error::Catalog(_)));
    assert!(err.to_string().contains("catalog error"));
}

#[test]
fn test_error_transaction() {
    let err = Error::transaction("tx failed");
    assert!(matches!(err, Error::Transaction(_)));
    assert!(err.to_string().contains("tx failed"));
}

#[test]
fn test_error_index() {
    let err = Error::index("index error");
    assert!(matches!(err, Error::Index(_)));
    assert!(err.to_string().contains("index error"));
}

#[test]
fn test_error_executor() {
    let err = Error::executor("exec error");
    assert!(matches!(err, Error::Executor(_)));
    assert!(err.to_string().contains("exec error"));
}

#[test]
fn test_error_internal() {
    let err = Error::internal("internal error");
    assert!(matches!(err, Error::Internal(_)));
    assert!(err.to_string().contains("internal error"));
}

#[test]
fn test_error_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err: Error = io_err.into();
    assert!(matches!(err, Error::Io(_)));
    assert!(err.to_string().contains("I/O error"));
}

#[test]
fn test_node_type_export() {
    // Test that NodeType is properly exported from the main library
    use crate::NodeType;

    let function = NodeType::Function;
    let module = NodeType::Module;
    let class = NodeType::Class;
    let variable = NodeType::Variable;
    let api = NodeType::API;

    // Test that all variants are accessible
    assert_eq!(format!("{:?}", function), "Function");
    assert_eq!(format!("{:?}", module), "Module");
    assert_eq!(format!("{:?}", class), "Class");
    assert_eq!(format!("{:?}", variable), "Variable");
    assert_eq!(format!("{:?}", api), "API");

    // Test serialization
    let json = serde_json::to_string(&api).unwrap();
    assert!(json.contains("API"));

    let deserialized: NodeType = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, NodeType::API);
}

#[test]
fn test_error_database() {
    let db_err = heed::Error::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "db file not found",
    ));
    let err: Error = db_err.into();
    assert!(matches!(err, Error::Database(_)));
}

#[test]
fn test_error_not_found() {
    let err = Error::NotFound("node 123".to_string());
    assert!(matches!(err, Error::NotFound(_)));
    assert!(err.to_string().contains("node 123"));
}

#[test]
fn test_error_invalid_id() {
    let err = Error::InvalidId("invalid node id".to_string());
    assert!(matches!(err, Error::InvalidId(_)));
    assert!(err.to_string().contains("invalid node id"));
}

#[test]
fn test_error_constraint_violation() {
    let err = Error::ConstraintViolation("unique constraint violated".to_string());
    assert!(matches!(err, Error::ConstraintViolation(_)));
    assert!(err.to_string().contains("unique constraint violated"));
}

#[test]
fn test_error_type_mismatch() {
    let err = Error::TypeMismatch {
        expected: "String".to_string(),
        actual: "Int64".to_string(),
    };
    assert!(matches!(err, Error::TypeMismatch { .. }));
    assert!(err.to_string().contains("String"));
    assert!(err.to_string().contains("Int64"));
}

#[test]
fn test_error_cypher_syntax() {
    let err = Error::CypherSyntax("unexpected token".to_string());
    assert!(matches!(err, Error::CypherSyntax(_)));
    assert!(err.to_string().contains("unexpected token"));
}

#[test]
fn test_error_debug() {
    let err = Error::Storage("test".to_string());
    let debug = format!("{:?}", err);
    assert!(debug.contains("Storage"));
}
