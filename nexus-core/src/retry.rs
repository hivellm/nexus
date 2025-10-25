//! Retry mechanisms for handling transient failures

use crate::{Error, Result};
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Jitter factor to randomize delays
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration
    pub fn new(max_attempts: u32, initial_delay: Duration) -> Self {
        Self {
            max_attempts,
            initial_delay,
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }

    /// Create a configuration for quick retries
    pub fn quick() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }

    /// Create a configuration for slow retries
    pub fn slow() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter_factor: 0.2,
        }
    }
}

/// Retry statistics
#[derive(Debug, Clone, Default)]
pub struct RetryStats {
    /// Total number of retry attempts
    pub total_attempts: u32,
    /// Number of successful retries
    pub successful_retries: u32,
    /// Number of failed retries
    pub failed_retries: u32,
    /// Total time spent retrying
    pub total_retry_time: Duration,
}

/// Retry context for tracking retry attempts
#[derive(Debug)]
pub struct RetryContext {
    config: RetryConfig,
    stats: RetryStats,
    start_time: Instant,
}

impl RetryContext {
    /// Create a new retry context
    pub fn new(config: RetryConfig) -> Self {
        Self {
            config,
            stats: RetryStats::default(),
            start_time: Instant::now(),
        }
    }

    /// Get current retry statistics
    pub fn stats(&self) -> RetryStats {
        let mut stats = self.stats.clone();
        stats.total_retry_time = self.start_time.elapsed();
        stats
    }
}

/// Execute a function with retry logic
pub async fn retry<F, Fut, T>(config: RetryConfig, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut context = RetryContext::new(config);
    let mut attempt = 0;

    loop {
        attempt += 1;
        context.stats.total_attempts = attempt;

        match operation().await {
            Ok(result) => {
                return Ok(result);
            }
            Err(error) => {
                // Check if this is a retryable error
                if !is_retryable(&error) {
                    return Err(error);
                }

                // Check if we've exceeded max attempts
                if attempt >= context.config.max_attempts {
                    return Err(error);
                }

                // Calculate delay with exponential backoff and jitter
                let delay = calculate_delay(&context.config, attempt);

                // Wait before retrying
                sleep(delay).await;
            }
        }
    }
}

/// Check if an error is retryable
fn is_retryable(error: &Error) -> bool {
    match error {
        Error::Retryable(_) => true,
        Error::Io(io_error) => {
            matches!(
                io_error.kind(),
                std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::WouldBlock
            )
        }
        Error::Database(db_error) => {
            // Check for transient LMDB errors by string matching
            let error_str = format!("{:?}", db_error);
            error_str.contains("MDB_MAP_FULL")
                || error_str.contains("MDB_TXN_FULL")
                || error_str.contains("MDB_READERS_FULL")
        }
        _ => false,
    }
}

/// Calculate delay with exponential backoff and jitter
fn calculate_delay(config: &RetryConfig, attempt: u32) -> Duration {
    // Calculate base delay with exponential backoff
    let base_delay =
        config.initial_delay.as_nanos() as f64 * config.backoff_multiplier.powi(attempt as i32 - 1);

    // Apply maximum delay limit
    let base_delay = base_delay.min(config.max_delay.as_nanos() as f64);

    // Add jitter to prevent thundering herd
    let jitter_range = base_delay * config.jitter_factor;
    let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;

    let final_delay = (base_delay + jitter).max(0.0) as u64;

    Duration::from_nanos(final_delay)
}

/// Retry a storage operation
pub async fn retry_storage<F, Fut, T>(operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    retry(RetryConfig::default(), operation).await
}

/// Retry a network operation
pub async fn retry_network<F, Fut, T>(operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    retry(RetryConfig::quick(), operation).await
}

/// Retry a database operation
pub async fn retry_database<F, Fut, T>(operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    retry(RetryConfig::slow(), operation).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(5));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.jitter_factor, 0.1);
    }

    #[test]
    fn test_retry_config_new() {
        let config = RetryConfig::new(5, Duration::from_millis(200));
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(200));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.jitter_factor, 0.1);
    }

    #[test]
    fn test_retry_config_quick() {
        let config = RetryConfig::quick();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(50));
        assert_eq!(config.max_delay, Duration::from_secs(1));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.jitter_factor, 0.1);
    }

    #[test]
    fn test_retry_config_slow() {
        let config = RetryConfig::slow();
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(500));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.jitter_factor, 0.2);
    }

    #[test]
    fn test_retry_stats_default() {
        let stats = RetryStats::default();
        assert_eq!(stats.total_attempts, 0);
        assert_eq!(stats.successful_retries, 0);
        assert_eq!(stats.failed_retries, 0);
        assert_eq!(stats.total_retry_time, Duration::from_secs(0));
    }

    #[test]
    fn test_retry_stats_clone() {
        let stats = RetryStats {
            total_attempts: 5,
            successful_retries: 3,
            failed_retries: 2,
            total_retry_time: Duration::from_millis(1000),
        };

        let cloned = stats.clone();
        assert_eq!(cloned.total_attempts, 5);
        assert_eq!(cloned.successful_retries, 3);
        assert_eq!(cloned.failed_retries, 2);
        assert_eq!(cloned.total_retry_time, Duration::from_millis(1000));
    }

    #[test]
    fn test_retry_context_new() {
        let config = RetryConfig::default();
        let context = RetryContext::new(config.clone());

        assert_eq!(context.config.max_attempts, config.max_attempts);
        assert_eq!(context.stats.total_attempts, 0);
    }

    #[test]
    fn test_retry_context_stats() {
        let config = RetryConfig::default();
        let context = RetryContext::new(config);

        let stats = context.stats();
        assert_eq!(stats.total_attempts, 0);
        assert_eq!(stats.successful_retries, 0);
        assert_eq!(stats.failed_retries, 0);
        assert!(stats.total_retry_time >= Duration::from_secs(0));
    }

    #[test]
    fn test_is_retryable_retryable_error() {
        let error = Error::Retryable("Test error".to_string());
        assert!(is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_io_timeout() {
        let error = Error::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "Timeout"));
        assert!(is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_io_interrupted() {
        let error = Error::Io(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Interrupted",
        ));
        assert!(is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_io_connection_refused() {
        let error = Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Connection refused",
        ));
        assert!(is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_io_connection_reset() {
        let error = Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "Connection reset",
        ));
        assert!(is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_io_connection_aborted() {
        let error = Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            "Connection aborted",
        ));
        assert!(is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_io_not_connected() {
        let error = Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "Not connected",
        ));
        assert!(is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_io_would_block() {
        let error = Error::Io(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "Would block",
        ));
        assert!(is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_io_not_retryable() {
        let error = Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Not found",
        ));
        assert!(!is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_database_retryable() {
        // Test that database errors with retryable patterns are detected
        // We'll use a simple approach by creating a heed::Error from a string
        let heed_error = heed::Error::Io(std::io::Error::other("MDB_MAP_FULL"));
        let error = Error::Database(heed_error);
        assert!(is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_database_not_retryable() {
        // Test that database errors without retryable patterns are not retryable
        let heed_error = heed::Error::Io(std::io::Error::other("MDB_INVALID"));
        let error = Error::Database(heed_error);
        assert!(!is_retryable(&error));
    }

    #[test]
    fn test_is_retryable_other_error() {
        let error = Error::Internal("Internal error".to_string());
        assert!(!is_retryable(&error));
    }

    #[test]
    fn test_calculate_delay_first_attempt() {
        let config = RetryConfig::default();
        let delay = calculate_delay(&config, 1);

        // First attempt should be close to initial delay (with jitter)
        assert!(delay >= Duration::from_millis(90)); // 100ms - 10% jitter
        assert!(delay <= Duration::from_millis(110)); // 100ms + 10% jitter
    }

    #[test]
    fn test_calculate_delay_second_attempt() {
        let config = RetryConfig::default();
        let delay = calculate_delay(&config, 2);

        // Second attempt should be around 200ms (with jitter)
        assert!(delay >= Duration::from_millis(180)); // 200ms - 10% jitter
        assert!(delay <= Duration::from_millis(220)); // 200ms + 10% jitter
    }

    #[test]
    fn test_calculate_delay_respects_max_delay() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(150),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        };

        let delay = calculate_delay(&config, 10); // High attempt number
        assert!(delay <= Duration::from_millis(165)); // 150ms + 10% jitter
    }

    #[test]
    fn test_calculate_delay_never_negative() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(150),
            backoff_multiplier: 2.0,
            jitter_factor: 1.0, // High jitter factor
        };

        let delay = calculate_delay(&config, 1);
        assert!(delay >= Duration::from_nanos(0));
    }

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig::quick();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry(config, move || {
            let call_count = call_count_clone.clone();
            async move {
                call_count.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, Error>(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_success_after_retries() {
        let config = RetryConfig::quick();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry(config, || {
            let call_count = call_count_clone.clone();
            async move {
                let count = call_count.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(Error::Retryable("Temporary failure".to_string()))
                } else {
                    Ok::<i32, Error>(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_max_attempts_exceeded() {
        let config = RetryConfig {
            max_attempts: 2,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
            backoff_multiplier: 1.0,
            jitter_factor: 0.0,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: Result<i32> = retry(config, move || {
            let call_count = call_count_clone.clone();
            async move {
                call_count.fetch_add(1, Ordering::SeqCst);
                Err(Error::Retryable("Always fails".to_string()))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_non_retryable_error() {
        let config = RetryConfig::quick();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result: Result<i32> = retry(config, move || {
            let call_count = call_count_clone.clone();
            async move {
                call_count.fetch_add(1, Ordering::SeqCst);
                Err(Error::Internal("Non-retryable error".to_string()))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_storage() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_storage(move || {
            let call_count = call_count_clone.clone();
            async move {
                call_count.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, Error>(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_network() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_network(move || {
            let call_count = call_count_clone.clone();
            async move {
                call_count.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, Error>(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_database() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_database(move || {
            let call_count = call_count_clone.clone();
            async move {
                call_count.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, Error>(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
