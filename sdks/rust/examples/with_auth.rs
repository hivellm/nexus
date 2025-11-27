//! Example using authentication with Nexus Rust SDK

use nexus_sdk::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Example 1: Using API key authentication
    tracing::info!("Example 1: API Key Authentication");
    let client = NexusClient::with_api_key("http://localhost:15474", "your-api-key")?;
    let healthy = client.health_check().await?;
    tracing::info!("Server is healthy: {}", healthy);

    // Example 2: Using username/password authentication
    tracing::info!("\nExample 2: Username/Password Authentication");
    let client = NexusClient::with_credentials("http://localhost:15474", "user", "pass")?;
    let _stats = client.get_stats().await?;
    tracing::info!("Database stats retrieved successfully");

    // Example 3: Using custom configuration
    tracing::info!("\nExample 3: Custom Configuration");
    // Note: ClientConfig is not public, use the builder pattern instead
    // For now, just use with_api_key or with_credentials
    let client = NexusClient::with_api_key("http://localhost:15474", "custom-api-key")?;
    let result = client.execute_cypher("RETURN 1 as test", None).await?;
    tracing::info!("Query executed successfully: {:?}", result);

    Ok(())
}
