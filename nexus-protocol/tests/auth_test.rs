//! Authentication tests for SDK clients

use nexus_protocol::{
    McpClient, McpClientError, RestClient, RestClientError, UmicpClient, UmicpClientError,
};

#[test]
fn test_rest_client_with_api_key() {
    let client = RestClient::with_api_key("http://localhost:8080", "nx_test123456789");
    assert_eq!(client.base_url(), "http://localhost:8080");
}

#[test]
fn test_mcp_client_with_api_key() {
    let client = McpClient::with_api_key("http://localhost:8080", "nx_test123456789");
    assert_eq!(client.endpoint(), "http://localhost:8080");
}

#[test]
fn test_umicp_client_with_api_key() {
    let client = UmicpClient::with_api_key("http://localhost:8080", "nx_test123456789");
    assert_eq!(client.endpoint(), "http://localhost:8080");
}

#[tokio::test]
async fn test_rest_client_key_rotation() {
    let mut client = RestClient::with_api_key("http://localhost:8080", "nx_old_key");
    client.rotate_key("nx_new_key").await;
    // Key rotation should succeed without panicking
}

#[tokio::test]
async fn test_mcp_client_key_rotation() {
    let mut client = McpClient::with_api_key("http://localhost:8080", "nx_old_key");
    client.rotate_key("nx_new_key").await;
    // Key rotation should succeed without panicking
}

#[tokio::test]
async fn test_umicp_client_key_rotation() {
    let mut client = UmicpClient::with_api_key("http://localhost:8080", "nx_old_key");
    client.rotate_key("nx_new_key").await;
    // Key rotation should succeed without panicking
}

#[test]
fn test_rest_client_error_types() {
    // Test that error types can be created and formatted
    let unauthorized = RestClientError::Unauthorized("Invalid API key".to_string());
    assert!(format!("{}", unauthorized).contains("Unauthorized"));

    let forbidden = RestClientError::Forbidden("Insufficient permissions".to_string());
    assert!(format!("{}", forbidden).contains("Forbidden"));

    let rate_limit = RestClientError::RateLimitExceeded("Too many requests".to_string());
    assert!(format!("{}", rate_limit).contains("Rate limit"));
}

#[test]
fn test_mcp_client_error_types() {
    // Test that error types can be created and formatted
    let unauthorized = McpClientError::Unauthorized("Invalid API key".to_string());
    assert!(format!("{}", unauthorized).contains("Unauthorized"));

    let forbidden = McpClientError::Forbidden("Insufficient permissions".to_string());
    assert!(format!("{}", forbidden).contains("Forbidden"));

    let rate_limit = McpClientError::RateLimitExceeded("Too many requests".to_string());
    assert!(format!("{}", rate_limit).contains("Rate limit"));

    let not_connected = McpClientError::NotConnected;
    assert!(format!("{}", not_connected).contains("not connected"));
}

#[test]
fn test_umicp_client_error_types() {
    // Test that error types can be created and formatted
    let unauthorized = UmicpClientError::Unauthorized("Invalid API key".to_string());
    assert!(format!("{}", unauthorized).contains("Unauthorized"));

    let forbidden = UmicpClientError::Forbidden("Insufficient permissions".to_string());
    assert!(format!("{}", forbidden).contains("Forbidden"));

    let rate_limit = UmicpClientError::RateLimitExceeded("Too many requests".to_string());
    assert!(format!("{}", rate_limit).contains("Rate limit"));
}

#[tokio::test]
async fn test_rest_client_without_api_key() {
    // Client without API key should still work (for unauthenticated endpoints)
    let client = RestClient::new("http://localhost:8080");
    assert_eq!(client.base_url(), "http://localhost:8080");
}

#[tokio::test]
async fn test_mcp_client_without_api_key() {
    // Client without API key should still work (for unauthenticated endpoints)
    let client = McpClient::new("http://localhost:8080");
    assert_eq!(client.endpoint(), "http://localhost:8080");
}

#[tokio::test]
async fn test_umicp_client_without_api_key() {
    // Client without API key should still work (for unauthenticated endpoints)
    let client = UmicpClient::new("http://localhost:8080");
    assert_eq!(client.endpoint(), "http://localhost:8080");
}
