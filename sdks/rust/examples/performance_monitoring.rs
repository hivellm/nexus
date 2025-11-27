//! Performance monitoring example for Nexus Rust SDK

use nexus_sdk::*;

#[tokio::main]
async fn main() -> Result<()> {
    let client = NexusClient::new("http://localhost:15474")?;

    tracing::info!("=== Performance Monitoring Example ===\n");

    // Get query statistics
    tracing::info!("1. Getting query statistics...");
    match client.get_query_statistics().await {
        Ok(stats) => {
            tracing::info!("   Total queries: {}", stats.statistics.total_queries);
            tracing::info!("   Successful: {}", stats.statistics.successful_queries);
            tracing::info!("   Failed: {}", stats.statistics.failed_queries);
            tracing::info!(
                "   Average execution time: {}ms",
                stats.statistics.average_execution_time_ms
            );
            tracing::info!("   Slow queries: {}", stats.statistics.slow_query_count);
            tracing::info!("   Patterns tracked: {}", stats.patterns.len());
        }
        Err(e) => tracing::info!("   Error: {}", e),
    }

    // Get slow queries
    tracing::info!("\n2. Getting slow queries...");
    match client.get_slow_queries().await {
        Ok(slow_queries) => {
            tracing::info!("   Found {} slow queries", slow_queries.count);
            for (i, query) in slow_queries.queries.iter().take(5).enumerate() {
                tracing::info!(
                    "   {}. {}ms - {}",
                    i + 1,
                    query.execution_time_ms,
                    query.query
                );
            }
        }
        Err(e) => tracing::info!("   Error: {}", e),
    }

    // Get plan cache statistics
    tracing::info!("\n3. Getting plan cache statistics...");
    match client.get_plan_cache_statistics().await {
        Ok(cache_stats) => {
            tracing::info!("   Cached plans: {}", cache_stats.cached_plans);
            tracing::info!("   Max size: {}", cache_stats.max_size);
            tracing::info!(
                "   Current memory: {} bytes",
                cache_stats.current_memory_bytes
            );
            tracing::info!("   Max memory: {} bytes", cache_stats.max_memory_bytes);
            tracing::info!("   Hit rate: {:.2}%", cache_stats.hit_rate * 100.0);
        }
        Err(e) => tracing::info!("   Error: {}", e),
    }

    // Clear plan cache
    tracing::info!("\n4. Clearing plan cache...");
    match client.clear_plan_cache().await {
        Ok(_) => tracing::info!("   Plan cache cleared successfully"),
        Err(e) => tracing::info!("   Error: {}", e),
    }

    tracing::info!("\n=== Example completed ===");
    Ok(())
}
