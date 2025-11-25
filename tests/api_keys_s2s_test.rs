//! End-to-end (S2S) tests for API Key Management via HTTP API
//!
//! These tests require the server to be running and are only executed when
//! the `s2s` feature is enabled.
//!
//! Usage:
//!   cargo test --features s2s --test api_keys_s2s_test
//!
//! Or set NEXUS_SERVER_URL environment variable to specify server URL:
//!   NEXUS_SERVER_URL=http://localhost:15474 cargo test --features s2s --test api_keys_s2s_test

#![cfg(feature = "s2s")]

use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing;

#[derive(Debug, Serialize, Deserialize)]
struct CypherRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CypherResponse {
    columns: Vec<String>,
    rows: Vec<serde_json::Value>,
    execution_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    username: Option<String>,
    permissions: Option<Vec<String>>,
    expires_in: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateApiKeyResponse {
    id: String,
    name: String,
    key: String,
    user_id: Option<String>,
    permissions: Vec<String>,
    created_at: String,
    expires_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiKeyResponse {
    id: String,
    name: String,
    user_id: Option<String>,
    permissions: Vec<String>,
    created_at: String,
    expires_at: Option<String>,
    is_active: bool,
    is_revoked: bool,
    revocation_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiKeysResponse {
    keys: Vec<ApiKeyResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RevokeApiKeyRequest {
    reason: Option<String>,
}

/// Get server URL from environment or use default
fn get_server_url() -> String {
    std::env::var("NEXUS_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:15474".to_string())
}

/// Check if server is available
async fn check_server_available(url: &str) -> bool {
    let client = reqwest::Client::new();
    client
        .get(&format!("{}/health", url))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Execute a Cypher query via HTTP API
async fn execute_query(
    client: &reqwest::Client,
    url: &str,
    query: &str,
) -> Result<CypherResponse, reqwest::Error> {
    let request = CypherRequest {
        query: query.to_string(),
        params: None,
    };

    let response = client
        .post(&format!("{}/cypher", url))
        .json(&request)
        .send()
        .await?;

    response.json().await
}

/// Test helper that expects success
async fn test_query_success(
    client: &reqwest::Client,
    url: &str,
    test_name: &str,
    query: &str,
) -> bool {
    match execute_query(client, url, query).await {
        Ok(response) => {
            if response.error.is_none() || response.error.as_ref().unwrap().is_empty() {
                tracing::info!("✓ {}: PASSED", test_name);
                true
            } else {
                tracing::info!("✗ {}: FAILED - Error: {:?}", test_name, response.error);
                false
            }
        }
        Err(e) => {
            tracing::info!("✗ {}: FAILED - Request error: {}", test_name, e);
            false
        }
    }
}

#[tokio::test]
async fn test_api_keys_s2s() {
    let server_url = get_server_url();

    // Check if server is available
    if !check_server_available(&server_url).await {
        etracing::info!("ERROR: Server not available at {}", server_url);
        etracing::info!("Please start the server first: cargo run --release --bin nexus-server");
        std::process::exit(1);
    }

    tracing::info!("Server is available at {}", server_url);
    tracing::info!("==========================================");
    tracing::info!("API Key Management S2S Tests");
    tracing::info!("==========================================");
    tracing::info!();

    let client = reqwest::Client::new();
    let mut passed = 0;
    let mut failed = 0;

    // Test CREATE API KEY via Cypher
    tracing::info!("--- CREATE API KEY (Cypher) Tests ---");
    if test_query_success(
        &client,
        &server_url,
        "CREATE API KEY basic",
        "CREATE API KEY testkey_s2s",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_success(
        &client,
        &server_url,
        "CREATE API KEY with permissions",
        "CREATE API KEY testkey_s2s_perm WITH PERMISSIONS READ, WRITE, ADMIN",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_success(
        &client,
        &server_url,
        "CREATE API KEY with expiration",
        "CREATE API KEY testkey_s2s_exp EXPIRES IN '7d'",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Create a user first for user association tests
    if test_query_success(
        &client,
        &server_url,
        "CREATE USER for API key tests",
        "CREATE USER testuser_s2s_keys",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_success(
        &client,
        &server_url,
        "CREATE API KEY FOR user",
        "CREATE API KEY testkey_s2s_user FOR testuser_s2s_keys",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test SHOW API KEYS via Cypher
    tracing::info!();
    tracing::info!("--- SHOW API KEYS (Cypher) Tests ---");
    if test_query_success(&client, &server_url, "SHOW API KEYS", "SHOW API KEYS").await {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_success(
        &client,
        &server_url,
        "SHOW API KEYS FOR user",
        "SHOW API KEYS FOR testuser_s2s_keys",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test REST API endpoints
    tracing::info!();
    tracing::info!("--- REST API Endpoints Tests ---");

    // POST /auth/keys - Create API key
    let create_request = CreateApiKeyRequest {
        name: "testkey_rest_s2s".to_string(),
        username: None,
        permissions: Some(vec!["READ".to_string(), "WRITE".to_string()]),
        expires_in: None,
    };

    match client
        .post(&format!("{}/auth/keys", server_url))
        .json(&create_request)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<CreateApiKeyResponse>().await {
                    Ok(api_key) => {
                        tracing::info!("✓ POST /auth/keys: PASSED");
                        assert!(api_key.key.starts_with("nx_"));
                        assert_eq!(api_key.name, "testkey_rest_s2s");
                        passed += 1;

                        // Test GET /auth/keys/{key_id}
                        match client
                            .get(&format!("{}/auth/keys/{}", server_url, api_key.id))
                            .send()
                            .await
                        {
                            Ok(get_response) => {
                                if get_response.status().is_success() {
                                    match get_response.json::<ApiKeyResponse>().await {
                                        Ok(key_info) => {
                                            tracing::info!("✓ GET /auth/keys/{{key_id}}: PASSED");
                                            assert_eq!(key_info.id, api_key.id);
                                            assert_eq!(key_info.name, api_key.name);
                                            passed += 1;
                                        }
                                        Err(e) => {
                                            tracing::info!("✗ GET /auth/keys/{{key_id}}: FAILED - Parse error: {}", e);
                                            failed += 1;
                                        }
                                    }
                                } else {
                                    tracing::info!(
                                        "✗ GET /auth/keys/{{key_id}}: FAILED - Status: {}",
                                        get_response.status()
                                    );
                                    failed += 1;
                                }
                            }
                            Err(e) => {
                                tracing::info!("✗ GET /auth/keys/{{key_id}}: FAILED - Request error: {}", e);
                                failed += 1;
                            }
                        }

                        // Test POST /auth/keys/{key_id}/revoke
                        let revoke_request = RevokeApiKeyRequest {
                            reason: Some("Test revocation".to_string()),
                        };

                        match client
                            .post(&format!("{}/auth/keys/{}/revoke", server_url, api_key.id))
                            .json(&revoke_request)
                            .send()
                            .await
                        {
                            Ok(revoke_response) => {
                                if revoke_response.status().is_success() {
                                    tracing::info!("✓ POST /auth/keys/{{key_id}}/revoke: PASSED");
                                    passed += 1;
                                } else {
                                    tracing::info!(
                                        "✗ POST /auth/keys/{{key_id}}/revoke: FAILED - Status: {}",
                                        revoke_response.status()
                                    );
                                    failed += 1;
                                }
                            }
                            Err(e) => {
                                tracing::info!(
                                    "✗ POST /auth/keys/{{key_id}}/revoke: FAILED - Request error: {}",
                                    e
                                );
                                failed += 1;
                            }
                        }

                        // Test DELETE /auth/keys/{key_id}
                        match client
                            .delete(&format!("{}/auth/keys/{}", server_url, api_key.id))
                            .send()
                            .await
                        {
                            Ok(delete_response) => {
                                if delete_response.status().is_success() {
                                    tracing::info!("✓ DELETE /auth/keys/{{key_id}}: PASSED");
                                    passed += 1;
                                } else {
                                    tracing::info!(
                                        "✗ DELETE /auth/keys/{{key_id}}: FAILED - Status: {}",
                                        delete_response.status()
                                    );
                                    failed += 1;
                                }
                            }
                            Err(e) => {
                                tracing::info!(
                                    "✗ DELETE /auth/keys/{{key_id}}: FAILED - Request error: {}",
                                    e
                                );
                                failed += 1;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::info!("✗ POST /auth/keys: FAILED - Parse error: {}", e);
                        failed += 1;
                    }
                }
            } else {
                tracing::info!("✗ POST /auth/keys: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("✗ POST /auth/keys: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test GET /auth/keys
    match client.get(&format!("{}/auth/keys", server_url)).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<ApiKeysResponse>().await {
                    Ok(keys_response) => {
                        tracing::info!("✓ GET /auth/keys: PASSED");
                        assert!(!keys_response.keys.is_empty());
                        passed += 1;
                    }
                    Err(e) => {
                        tracing::info!("✗ GET /auth/keys: FAILED - Parse error: {}", e);
                        failed += 1;
                    }
                }
            } else {
                tracing::info!("✗ GET /auth/keys: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("✗ GET /auth/keys: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test GET /auth/keys?username=...
    match client
        .get(&format!("{}/auth/keys?username=testuser_s2s_keys", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<ApiKeysResponse>().await {
                    Ok(keys_response) => {
                        tracing::info!("✓ GET /auth/keys?username=...: PASSED");
                        passed += 1;
                    }
                    Err(e) => {
                        tracing::info!("✗ GET /auth/keys?username=...: FAILED - Parse error: {}", e);
                        failed += 1;
                    }
                }
            } else {
                tracing::info!("✗ GET /auth/keys?username=...: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("✗ GET /auth/keys?username=...: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Summary
    tracing::info!();
    tracing::info!("==========================================");
    tracing::info!("Test Summary");
    tracing::info!("==========================================");
    tracing::info!("Passed: {}", passed);
    tracing::info!("Failed: {}", failed);
    tracing::info!("Total:  {}", passed + failed);
    tracing::info!();

    if failed > 0 {
        etracing::info!("Some tests failed!");
        std::process::exit(1);
    }
}

