//! Security and rate limiting module for Nexus

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per minute
    pub per_minute: u32,
    /// Requests per hour
    pub per_hour: u32,
    /// Requests per day
    pub per_day: u32,
    /// Burst allowance (extra requests allowed in short bursts)
    pub burst_allowance: u32,
    /// Window size for rate limiting (seconds)
    pub window_size: u64,
    /// Whether to enable rate limiting
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_minute: 1000,
            per_hour: 10000,
            per_day: 100000,
            burst_allowance: 100,
            window_size: 60,
            enabled: true,
        }
    }
}

/// Rate limit entry for tracking requests
#[derive(Debug, Clone)]
struct RateLimitEntry {
    /// Number of requests in current window
    requests: u32,
    /// Window start time
    window_start: Instant,
    /// Last request time
    last_request: Instant,
    /// Burst allowance used
    burst_used: u32,
}

impl RateLimitEntry {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            requests: 0,
            window_start: now,
            last_request: now,
            burst_used: 0,
        }
    }

    fn reset_window(&mut self) {
        let now = Instant::now();
        self.requests = 0;
        self.window_start = now;
        self.burst_used = 0;
    }

    fn can_make_request(&self, config: &RateLimitConfig) -> bool {
        if !config.enabled {
            return true;
        }

        let now = Instant::now();
        let window_elapsed = now.duration_since(self.window_start).as_secs();

        // Reset window if it's expired
        if window_elapsed >= config.window_size {
            return true;
        }

        // Check if we're within limits (use per_minute as the primary limit)
        self.requests < config.per_minute
    }

    fn add_request(&mut self, config: &RateLimitConfig) {
        let now = Instant::now();
        let window_elapsed = now.duration_since(self.window_start).as_secs();

        // Reset window if it's expired
        if window_elapsed >= config.window_size {
            self.reset_window();
        }

        self.requests += 1;
        self.last_request = now;
    }
}

/// Rate limiter for tracking and enforcing rate limits
#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    entries: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
    cleanup_interval: tokio::time::Interval,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        let mut cleanup_interval = interval(Duration::from_secs(300)); // Cleanup every 5 minutes
        cleanup_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        Self {
            config,
            entries: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval,
        }
    }

    /// Check if a request is allowed for the given key
    pub async fn is_allowed(&self, key: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        let mut entries = self.entries.write().await;
        let entry = entries
            .entry(key.to_string())
            .or_insert_with(RateLimitEntry::new);

        if entry.can_make_request(&self.config) {
            entry.add_request(&self.config);
            true
        } else {
            false
        }
    }

    /// Get the current request count for a key
    pub async fn get_request_count(&self, key: &str) -> u32 {
        let entries = self.entries.read().await;
        entries.get(key).map(|e| e.requests).unwrap_or(0)
    }

    /// Reset the rate limit for a key
    pub async fn reset_key(&self, key: &str) {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            entry.reset_window();
        }
    }

    /// Get rate limit statistics
    pub async fn get_stats(&self) -> RateLimitStats {
        let entries = self.entries.read().await;
        let total_keys = entries.len();
        let active_keys = entries.values().filter(|e| e.requests > 0).count();

        RateLimitStats {
            total_keys,
            active_keys,
            config: self.config.clone(),
        }
    }

    /// Start the cleanup task
    pub fn start_cleanup_task(mut self) {
        let entries = self.entries.clone();
        let window_size = self.config.window_size;

        tokio::spawn(async move {
            loop {
                // Wait for the next cleanup interval
                self.cleanup_interval.tick().await;

                // Clean up expired entries
                let mut entries = entries.write().await;
                let now = Instant::now();
                entries.retain(|_, entry| {
                    now.duration_since(entry.window_start).as_secs() < window_size * 2
                });
            }
        });
    }
}

/// Rate limit statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStats {
    pub total_keys: usize,
    pub active_keys: usize,
    pub config: RateLimitConfig,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Rate limiting configuration
    pub rate_limits: RateLimitConfig,
    /// Maximum request size (bytes)
    pub max_request_size: usize,
    /// Maximum query complexity
    pub max_query_complexity: u32,
    /// Enable request validation
    pub enable_validation: bool,
    /// Enable SQL injection protection
    pub enable_sql_injection_protection: bool,
    /// Enable XSS protection
    pub enable_xss_protection: bool,
    /// Allowed origins for CORS
    pub allowed_origins: Vec<String>,
    /// Enable HTTPS only
    pub require_https: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            rate_limits: RateLimitConfig::default(),
            max_request_size: 10 * 1024 * 1024, // 10MB
            max_query_complexity: 1000,
            enable_validation: true,
            enable_sql_injection_protection: true,
            enable_xss_protection: true,
            allowed_origins: vec!["*".to_string()],
            require_https: false,
        }
    }
}

/// Security manager for handling all security-related operations
#[derive(Debug)]
pub struct SecurityManager {
    config: SecurityConfig,
    rate_limiter: RateLimiter,
    blocked_ips: Arc<RwLock<HashMap<String, Instant>>>,
    suspicious_ips: Arc<RwLock<HashMap<String, u32>>>,
}

impl SecurityManager {
    /// Create a new security manager
    pub fn new(config: SecurityConfig) -> Self {
        Self {
            rate_limiter: RateLimiter::new(config.rate_limits.clone()),
            config,
            blocked_ips: Arc::new(RwLock::new(HashMap::new())),
            suspicious_ips: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a request is allowed based on rate limits and security rules
    pub async fn is_request_allowed(&self, key: &str, ip: &str) -> Result<bool> {
        // Check if IP is blocked
        if self.is_ip_blocked(ip).await {
            return Ok(false);
        }

        // Check rate limits
        if !self.rate_limiter.is_allowed(key).await {
            // Mark IP as suspicious
            self.mark_suspicious_ip(ip).await;
            return Ok(false);
        }

        Ok(true)
    }

    /// Check if an IP is blocked
    pub async fn is_ip_blocked(&self, ip: &str) -> bool {
        let blocked_ips = self.blocked_ips.read().await;
        if let Some(blocked_until) = blocked_ips.get(ip) {
            if Instant::now() < *blocked_until {
                return true;
            }
        }
        false
    }

    /// Block an IP address
    pub async fn block_ip(&self, ip: &str, duration: Duration) {
        let mut blocked_ips = self.blocked_ips.write().await;
        blocked_ips.insert(ip.to_string(), Instant::now() + duration);
    }

    /// Mark an IP as suspicious
    pub async fn mark_suspicious_ip(&self, ip: &str) {
        let mut suspicious_ips = self.suspicious_ips.write().await;
        let count = suspicious_ips.entry(ip.to_string()).or_insert(0);
        *count += 1;

        // Block IP if it's been marked suspicious too many times
        if *count >= 5 {
            self.block_ip(ip, Duration::from_secs(3600)).await;
        }
    }

    /// Validate a request for security issues
    pub fn validate_request(&self, request: &str) -> Result<()> {
        if !self.config.enable_validation {
            return Ok(());
        }

        // Check request size
        if request.len() > self.config.max_request_size {
            return Err(Error::internal("Request too large"));
        }

        // Check for SQL injection patterns
        if self.config.enable_sql_injection_protection
            && self.contains_sql_injection_patterns(request)
        {
            return Err(Error::internal("Potential SQL injection detected"));
        }

        // Check for XSS patterns
        if self.config.enable_xss_protection && self.contains_xss_patterns(request) {
            return Err(Error::internal("Potential XSS detected"));
        }

        Ok(())
    }

    /// Check for SQL injection patterns
    fn contains_sql_injection_patterns(&self, input: &str) -> bool {
        let patterns = [
            "'; drop table",
            "union select",
            "or 1=1",
            "and 1=1",
            "'; --",
            "/*",
            "*/",
            "xp_",
            "sp_",
            "exec",
            "execute",
        ];

        let input_lower = input.to_lowercase();
        patterns.iter().any(|pattern| input_lower.contains(pattern))
    }

    /// Check for XSS patterns
    fn contains_xss_patterns(&self, input: &str) -> bool {
        let patterns = [
            "<script",
            "javascript:",
            "onload=",
            "onerror=",
            "onclick=",
            "onmouseover=",
            "onfocus=",
            "onblur=",
            "onchange=",
            "onsubmit=",
        ];

        let input_lower = input.to_lowercase();
        patterns.iter().any(|pattern| input_lower.contains(pattern))
    }

    /// Get security statistics
    pub async fn get_stats(&self) -> SecurityStats {
        let rate_limit_stats = self.rate_limiter.get_stats().await;
        let blocked_ips = self.blocked_ips.read().await;
        let suspicious_ips = self.suspicious_ips.read().await;

        SecurityStats {
            rate_limits: rate_limit_stats,
            blocked_ips_count: blocked_ips.len(),
            suspicious_ips_count: suspicious_ips.len(),
            config: self.config.clone(),
        }
    }

    /// Get the rate limiter
    pub fn rate_limiter(&self) -> &RateLimiter {
        &self.rate_limiter
    }

    /// Get the security configuration
    pub fn config(&self) -> &SecurityConfig {
        &self.config
    }
}

/// Security statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityStats {
    pub rate_limits: RateLimitStats,
    pub blocked_ips_count: usize,
    pub suspicious_ips_count: usize,
    pub config: SecurityConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_rate_limiter_creation() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        let stats = limiter.get_stats().await;
        assert_eq!(stats.total_keys, 0);
        assert_eq!(stats.active_keys, 0);
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_requests() {
        let config = RateLimitConfig {
            per_minute: 10,
            per_hour: 100,
            per_day: 1000,
            burst_allowance: 5,
            window_size: 60,
            enabled: true,
        };
        let limiter = RateLimiter::new(config);

        // Should allow requests within limits
        for _ in 0..10 {
            assert!(limiter.is_allowed("test_key").await);
        }

        // Should block requests beyond limits
        assert!(!limiter.is_allowed("test_key").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_reset() {
        let config = RateLimitConfig {
            per_minute: 5,
            per_hour: 100,
            per_day: 1000,
            burst_allowance: 2,
            window_size: 1, // 1 second window for testing
            enabled: true,
        };
        let limiter = RateLimiter::new(config);

        // Use up the limit
        for _ in 0..5 {
            assert!(limiter.is_allowed("test_key").await);
        }

        // Should be blocked
        assert!(!limiter.is_allowed("test_key").await);

        // Wait for window to reset
        sleep(Duration::from_secs(2)).await;

        // Should be allowed again
        assert!(limiter.is_allowed("test_key").await);
    }

    #[tokio::test]
    async fn test_security_manager_creation() {
        let config = SecurityConfig::default();
        let manager = SecurityManager::new(config);

        let stats = manager.get_stats().await;
        assert_eq!(stats.blocked_ips_count, 0);
        assert_eq!(stats.suspicious_ips_count, 0);
    }

    #[tokio::test]
    async fn test_ip_blocking() {
        let config = SecurityConfig::default();
        let manager = SecurityManager::new(config);

        // IP should not be blocked initially
        assert!(!manager.is_ip_blocked("192.168.1.1").await);

        // Block the IP
        manager
            .block_ip("192.168.1.1", Duration::from_secs(1))
            .await;

        // IP should be blocked
        assert!(manager.is_ip_blocked("192.168.1.1").await);

        // Wait for block to expire
        sleep(Duration::from_secs(2)).await;

        // IP should not be blocked anymore
        assert!(!manager.is_ip_blocked("192.168.1.1").await);
    }

    #[tokio::test]
    async fn test_suspicious_ip_tracking() {
        let config = SecurityConfig::default();
        let manager = SecurityManager::new(config);

        // Mark IP as suspicious multiple times
        for _ in 0..5 {
            manager.mark_suspicious_ip("192.168.1.1").await;
        }

        // IP should be blocked after 5 suspicious activities
        assert!(manager.is_ip_blocked("192.168.1.1").await);
    }

    #[tokio::test]
    async fn test_sql_injection_detection() {
        let config = SecurityConfig::default();
        let manager = SecurityManager::new(config);

        // Should detect SQL injection patterns
        assert!(manager.contains_sql_injection_patterns("'; DROP TABLE users; --"));
        assert!(manager.contains_sql_injection_patterns("UNION SELECT * FROM users"));
        assert!(manager.contains_sql_injection_patterns("OR 1=1"));

        // Should not detect normal queries
        assert!(!manager.contains_sql_injection_patterns("SELECT * FROM users WHERE id = 1"));
    }

    #[tokio::test]
    async fn test_xss_detection() {
        let config = SecurityConfig::default();
        let manager = SecurityManager::new(config);

        // Should detect XSS patterns
        assert!(manager.contains_xss_patterns("<script>alert('xss')</script>"));
        assert!(manager.contains_xss_patterns("javascript:alert('xss')"));
        assert!(manager.contains_xss_patterns("onload=alert('xss')"));

        // Should not detect normal content
        assert!(!manager.contains_xss_patterns("Hello, world!"));
    }

    #[tokio::test]
    async fn test_request_validation() {
        let config = SecurityConfig {
            max_request_size: 100,
            enable_validation: true,
            enable_sql_injection_protection: true,
            enable_xss_protection: true,
            ..Default::default()
        };
        let manager = SecurityManager::new(config);

        // Should validate normal request
        assert!(manager.validate_request("SELECT * FROM users").is_ok());

        // Should reject large request
        let large_request = "x".repeat(200);
        assert!(manager.validate_request(&large_request).is_err());

        // Should reject SQL injection
        assert!(manager.validate_request("'; DROP TABLE users; --").is_err());

        // Should reject XSS
        assert!(
            manager
                .validate_request("<script>alert('xss')</script>")
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_request_allowed() {
        let config = SecurityConfig::default();
        let manager = SecurityManager::new(config);

        // Should allow normal request
        assert!(
            manager
                .is_request_allowed("test_key", "192.168.1.1")
                .await
                .unwrap()
        );

        // Should block if IP is blocked
        manager
            .block_ip("192.168.1.1", Duration::from_secs(1))
            .await;
        assert!(
            !manager
                .is_request_allowed("test_key", "192.168.1.1")
                .await
                .unwrap()
        );
    }
}
