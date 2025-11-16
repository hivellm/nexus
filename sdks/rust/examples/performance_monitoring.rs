//! Performance monitoring example for Nexus Rust SDK

use nexus_sdk::*;

#[tokio::main]
async fn main() -> Result<()> {
    let client = NexusClient::new("http://localhost:15474")?;

    println!("=== Performance Monitoring Example ===\n");

    // Get query statistics
    println!("1. Getting query statistics...");
    match client.get_query_statistics().await {
        Ok(stats) => {
            println!("   Total queries: {}", stats.statistics.total_queries);
            println!("   Successful: {}", stats.statistics.successful_queries);
            println!("   Failed: {}", stats.statistics.failed_queries);
            println!(
                "   Average execution time: {}ms",
                stats.statistics.average_execution_time_ms
            );
            println!("   Slow queries: {}", stats.statistics.slow_query_count);
            println!("   Patterns tracked: {}", stats.patterns.len());
        }
        Err(e) => println!("   Error: {}", e),
    }

    // Get slow queries
    println!("\n2. Getting slow queries...");
    match client.get_slow_queries().await {
        Ok(slow_queries) => {
            println!("   Found {} slow queries", slow_queries.count);
            for (i, query) in slow_queries.queries.iter().take(5).enumerate() {
                println!(
                    "   {}. {}ms - {}",
                    i + 1,
                    query.execution_time_ms,
                    query.query
                );
            }
        }
        Err(e) => println!("   Error: {}", e),
    }

    // Get plan cache statistics
    println!("\n3. Getting plan cache statistics...");
    match client.get_plan_cache_statistics().await {
        Ok(cache_stats) => {
            println!("   Cached plans: {}", cache_stats.cached_plans);
            println!("   Max size: {}", cache_stats.max_size);
            println!(
                "   Current memory: {} bytes",
                cache_stats.current_memory_bytes
            );
            println!("   Max memory: {} bytes", cache_stats.max_memory_bytes);
            println!("   Hit rate: {:.2}%", cache_stats.hit_rate * 100.0);
        }
        Err(e) => println!("   Error: {}", e),
    }

    // Clear plan cache
    println!("\n4. Clearing plan cache...");
    match client.clear_plan_cache().await {
        Ok(_) => println!("   Plan cache cleared successfully"),
        Err(e) => println!("   Error: {}", e),
    }

    println!("\n=== Example completed ===");
    Ok(())
}
