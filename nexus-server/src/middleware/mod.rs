//! Middleware modules

pub mod auth;
pub mod mcp_auth;
pub mod rate_limit;

pub use auth::{create_auth_middleware, route_requires_auth};
pub use mcp_auth::mcp_auth_middleware_handler;
pub use rate_limit::{RateLimitConfig, RateLimiter, rate_limit_middleware};
