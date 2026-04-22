//! SDK endpoint URL parsing — mirror of `nexus-cli/src/endpoint.rs`.
//!
//! The CLI and the Rust SDK share the same URL grammar so users can
//! copy-paste endpoints between `nexus --url <URL>` and
//! `NexusClient::new(<URL>)` without surprises.

use crate::error::{NexusError, Result};
use std::fmt;

/// Which wire protocol the endpoint uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Native binary RPC (length-prefixed MessagePack over TCP,
    /// port `15475` by default).
    Rpc,
    /// HTTP/JSON (port `15474` by default).
    Http,
    /// HTTPS/JSON (port `443` by default).
    Https,
    /// RESP3 (port `15476` by default). Parsed but not yet served by
    /// this SDK.
    Resp3,
}

impl fmt::Display for Scheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rpc => f.write_str("nexus"),
            Self::Http => f.write_str("http"),
            Self::Https => f.write_str("https"),
            Self::Resp3 => f.write_str("resp3"),
        }
    }
}

pub const RPC_DEFAULT_PORT: u16 = 15475;
pub const HTTP_DEFAULT_PORT: u16 = 15474;
pub const HTTPS_DEFAULT_PORT: u16 = 443;
pub const RESP3_DEFAULT_PORT: u16 = 15476;

/// A parsed endpoint URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    /// Default loopback endpoint — `nexus://127.0.0.1:15475`.
    pub fn default_local() -> Self {
        Self {
            scheme: Scheme::Rpc,
            host: "127.0.0.1".to_string(),
            port: RPC_DEFAULT_PORT,
        }
    }

    /// Parse any of the four accepted URL forms. Returns
    /// `NexusError::Configuration` for unknown schemes, empty input,
    /// malformed ports, etc.
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(NexusError::Configuration(
                "endpoint URL must not be empty".to_string(),
            ));
        }

        if let Some((scheme, rest)) = trimmed.split_once("://") {
            let rest = rest.trim_end_matches('/');
            let (host, port) = split_host_port(rest)?;
            let (scheme, default_port) = match scheme.to_ascii_lowercase().as_str() {
                "nexus" => (Scheme::Rpc, RPC_DEFAULT_PORT),
                "http" => (Scheme::Http, HTTP_DEFAULT_PORT),
                "https" => (Scheme::Https, HTTPS_DEFAULT_PORT),
                "resp3" => (Scheme::Resp3, RESP3_DEFAULT_PORT),
                other => {
                    return Err(NexusError::Configuration(format!(
                        "unsupported URL scheme '{}://' (expected 'nexus://', 'http://', \
                         'https://', or 'resp3://')",
                        other
                    )));
                }
            };
            Ok(Self {
                scheme,
                host,
                port: port.unwrap_or(default_port),
            })
        } else {
            // Bare form: `host` or `host:port` → treat as RPC.
            let (host, port) = split_host_port(trimmed)?;
            Ok(Self {
                scheme: Scheme::Rpc,
                host,
                port: port.unwrap_or(RPC_DEFAULT_PORT),
            })
        }
    }

    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Render the endpoint as an HTTP URL suitable for `reqwest`.
    /// Translates `nexus://` and `resp3://` into the sibling HTTP
    /// port so the HTTP fallback always has a URL to hit.
    pub fn as_http_url(&self) -> String {
        match self.scheme {
            Scheme::Http => format!("http://{}", self.authority()),
            Scheme::Https => format!("https://{}", self.authority()),
            Scheme::Rpc | Scheme::Resp3 => {
                format!("http://{}:{}", self.host, HTTP_DEFAULT_PORT)
            }
        }
    }

    pub fn is_rpc(&self) -> bool {
        matches!(self.scheme, Scheme::Rpc)
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.scheme, self.authority())
    }
}

fn split_host_port(s: &str) -> Result<(String, Option<u16>)> {
    if s.is_empty() {
        return Err(NexusError::Configuration("missing host".to_string()));
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (host, tail) = rest.split_once(']').ok_or_else(|| {
            NexusError::Configuration(format!("unterminated IPv6 literal in '{}'", s))
        })?;
        let port = if let Some(port_str) = tail.strip_prefix(':') {
            Some(parse_port(port_str)?)
        } else if tail.is_empty() {
            None
        } else {
            return Err(NexusError::Configuration(format!(
                "unexpected characters after IPv6 literal: '{}'",
                tail
            )));
        };
        return Ok((host.to_string(), port));
    }
    if let Some((host, port_str)) = s.rsplit_once(':') {
        if host.is_empty() {
            return Err(NexusError::Configuration(format!(
                "missing host in '{}'",
                s
            )));
        }
        return Ok((host.to_string(), Some(parse_port(port_str)?)));
    }
    Ok((s.to_string(), None))
}

fn parse_port(s: &str) -> Result<u16> {
    s.parse::<u16>()
        .map_err(|_| NexusError::Configuration(format!("invalid port '{}': must be 0..=65535", s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_local_is_nexus_loopback() {
        let ep = Endpoint::default_local();
        assert_eq!(ep.scheme, Scheme::Rpc);
        assert_eq!(ep.port, 15475);
        assert_eq!(ep.to_string(), "nexus://127.0.0.1:15475");
    }

    #[test]
    fn parse_nexus_with_port() {
        let ep = Endpoint::parse("nexus://example.com:17000").unwrap();
        assert_eq!(ep.scheme, Scheme::Rpc);
        assert_eq!(ep.host, "example.com");
        assert_eq!(ep.port, 17000);
    }

    #[test]
    fn parse_nexus_default_port() {
        let ep = Endpoint::parse("nexus://db.internal").unwrap();
        assert_eq!(ep.port, RPC_DEFAULT_PORT);
    }

    #[test]
    fn parse_http_default_port() {
        let ep = Endpoint::parse("http://localhost").unwrap();
        assert_eq!(ep.scheme, Scheme::Http);
        assert_eq!(ep.port, HTTP_DEFAULT_PORT);
    }

    #[test]
    fn parse_https_default_port() {
        let ep = Endpoint::parse("https://nexus.example.com").unwrap();
        assert_eq!(ep.scheme, Scheme::Https);
        assert_eq!(ep.port, HTTPS_DEFAULT_PORT);
    }

    #[test]
    fn parse_bare_form_is_rpc() {
        let ep = Endpoint::parse("10.0.0.5:15600").unwrap();
        assert_eq!(ep.scheme, Scheme::Rpc);
        assert_eq!(ep.port, 15600);
    }

    #[test]
    fn parse_ipv6_with_port() {
        let ep = Endpoint::parse("nexus://[::1]:15475").unwrap();
        assert_eq!(ep.host, "::1");
        assert_eq!(ep.port, 15475);
    }

    #[test]
    fn rejects_nexus_rpc_scheme() {
        let err = Endpoint::parse("nexus-rpc://host").unwrap_err();
        assert!(format!("{err}").contains("unsupported URL scheme"));
    }

    #[test]
    fn rejects_empty() {
        assert!(Endpoint::parse("").is_err());
        assert!(Endpoint::parse("   ").is_err());
    }

    #[test]
    fn as_http_url_swaps_rpc_to_sibling_http_port() {
        let ep = Endpoint::parse("nexus://host:17000").unwrap();
        assert_eq!(ep.as_http_url(), "http://host:15474");
    }
}
