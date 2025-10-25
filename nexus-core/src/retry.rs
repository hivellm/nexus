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
            // Check for transient LMDB errors
            format!("{:?}", db_error).contains("MDB_MAP_FULL")
                || format!("{:?}", db_error).contains("MDB_TXN_FULL")
                || format!("{:?}", db_error).contains("MDB_READERS_FULL")
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
