//! Endpoint URL parsing for the CLI.
//!
//! The CLI accepts three URL forms:
//!
//! - `nexus://host[:port]` — **default**. Binary RPC on the Nexus wire
//!   format. Port defaults to `15475`.
//! - `http://host[:port]` / `https://host[:port]` — legacy HTTP/JSON
//!   transport. Port defaults to `15474` (or `443` for https).
//! - `host[:port]` — bare form; interpreted as `nexus://` so the CLI
//!   stays RPC-first by default.
//!
//! There is no `nexus-rpc://` or `nexus+rpc://` scheme — the canonical
//! single-token scheme is `nexus`.

use anyhow::{Result, anyhow, bail};
use std::fmt;

/// Which wire protocol the endpoint uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Native binary RPC (length-prefixed MessagePack over TCP, port
    /// `15475` by default).
    Rpc,
    /// HTTP/JSON (port `15474` by default).
    Http,
    /// HTTPS/JSON (port `443` by default).
    Https,
}

impl fmt::Display for Scheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rpc => f.write_str("nexus"),
            Self::Http => f.write_str("http"),
            Self::Https => f.write_str("https"),
        }
    }
}

/// Well-known default ports for each scheme.
///
/// These must match the server defaults — see
/// `nexus-server/src/main.rs` (`15474` HTTP) and the phase1 RPC listener
/// (`15475`). Changing either requires updating both.
pub const RPC_DEFAULT_PORT: u16 = 15475;
pub const HTTP_DEFAULT_PORT: u16 = 15474;
pub const HTTPS_DEFAULT_PORT: u16 = 443;

/// A parsed endpoint URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    /// The default endpoint used when the caller does not supply one —
    /// `nexus://127.0.0.1:15475`.
    pub fn default_local() -> Self {
        Self {
            scheme: Scheme::Rpc,
            host: "127.0.0.1".to_string(),
            port: RPC_DEFAULT_PORT,
        }
    }

    /// Parse a user-supplied URL. Accepts any of the three forms the
    /// module doc describes.
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("endpoint URL must not be empty");
        }

        // Look for a scheme.
        if let Some((scheme, rest)) = trimmed.split_once("://") {
            let rest = rest.trim_end_matches('/');
            let (host, port) = split_host_port(rest)?;
            let (scheme, default_port) = match scheme.to_ascii_lowercase().as_str() {
                "nexus" => (Scheme::Rpc, RPC_DEFAULT_PORT),
                "http" => (Scheme::Http, HTTP_DEFAULT_PORT),
                "https" => (Scheme::Https, HTTPS_DEFAULT_PORT),
                // Reject schemes we explicitly do not recognise so a
                // typo like `nexus-rpc://` fails loud instead of being
                // silently misinterpreted.
                other => bail!(
                    "unsupported URL scheme '{}://' (did you mean 'nexus://', 'http://', or 'https://'?)",
                    other
                ),
            };
            Ok(Self {
                scheme,
                host,
                port: port.unwrap_or(default_port),
            })
        } else {
            // Bare form: `host` or `host:port`. Treat as RPC — matches
            // the CLI's RPC-first default.
            let (host, port) = split_host_port(trimmed)?;
            Ok(Self {
                scheme: Scheme::Rpc,
                host,
                port: port.unwrap_or(RPC_DEFAULT_PORT),
            })
        }
    }

    /// `host:port` string suitable for `TcpStream::connect` / `reqwest`
    /// URL construction.
    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Render the endpoint as an HTTP URL, translating `nexus://` into
    /// the sibling HTTP port (`15474`). Used by HTTP-only operations
    /// that the CLI falls back to when no RPC verb exists yet (export /
    /// import / auth-admin, etc.).
    pub fn as_http_url(&self) -> String {
        match self.scheme {
            Scheme::Http => format!("http://{}", self.authority()),
            Scheme::Https => format!("https://{}", self.authority()),
            Scheme::Rpc => {
                // Swap to the sibling HTTP port by convention.
                format!("http://{}:{}", self.host, HTTP_DEFAULT_PORT)
            }
        }
    }

    /// True when the endpoint uses the native RPC wire format.
    pub fn is_rpc(&self) -> bool {
        matches!(self.scheme, Scheme::Rpc)
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.scheme, self.authority())
    }
}

/// Split a `host[:port]` token. Empty host is rejected.
fn split_host_port(s: &str) -> Result<(String, Option<u16>)> {
    if s.is_empty() {
        bail!("missing host");
    }
    // IPv6 literal: `[::1]:15475`
    if let Some(rest) = s.strip_prefix('[') {
        let (host, tail) = rest
            .split_once(']')
            .ok_or_else(|| anyhow!("unterminated IPv6 literal in '{}'", s))?;
        let port = if let Some(port_str) = tail.strip_prefix(':') {
            Some(parse_port(port_str)?)
        } else if tail.is_empty() {
            None
        } else {
            bail!("unexpected characters after IPv6 literal: '{}'", tail);
        };
        return Ok((host.to_string(), port));
    }
    // Plain form: split on the LAST colon so numeric IPv4:port parses
    // cleanly.
    if let Some((host, port_str)) = s.rsplit_once(':') {
        if host.is_empty() {
            bail!("missing host in '{}'", s);
        }
        return Ok((host.to_string(), Some(parse_port(port_str)?)));
    }
    Ok((s.to_string(), None))
}

fn parse_port(s: &str) -> Result<u16> {
    s.parse::<u16>()
        .map_err(|_| anyhow!("invalid port '{}': must be 0..=65535", s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_local_is_nexus_loopback_on_15475() {
        let ep = Endpoint::default_local();
        assert_eq!(ep.scheme, Scheme::Rpc);
        assert_eq!(ep.host, "127.0.0.1");
        assert_eq!(ep.port, 15475);
        assert_eq!(ep.to_string(), "nexus://127.0.0.1:15475");
    }

    #[test]
    fn parse_nexus_scheme_with_explicit_port() {
        let ep = Endpoint::parse("nexus://example.com:17000").unwrap();
        assert_eq!(ep.scheme, Scheme::Rpc);
        assert_eq!(ep.host, "example.com");
        assert_eq!(ep.port, 17000);
    }

    #[test]
    fn parse_nexus_scheme_with_default_port() {
        let ep = Endpoint::parse("nexus://db.internal").unwrap();
        assert_eq!(ep.port, RPC_DEFAULT_PORT);
    }

    #[test]
    fn parse_http_scheme_with_default_port() {
        let ep = Endpoint::parse("http://localhost").unwrap();
        assert_eq!(ep.scheme, Scheme::Http);
        assert_eq!(ep.port, HTTP_DEFAULT_PORT);
    }

    #[test]
    fn parse_https_scheme_with_default_port() {
        let ep = Endpoint::parse("https://nexus.example.com").unwrap();
        assert_eq!(ep.scheme, Scheme::Https);
        assert_eq!(ep.port, HTTPS_DEFAULT_PORT);
    }

    #[test]
    fn parse_bare_form_is_rpc() {
        let ep = Endpoint::parse("10.0.0.5:15600").unwrap();
        assert_eq!(ep.scheme, Scheme::Rpc);
        assert_eq!(ep.host, "10.0.0.5");
        assert_eq!(ep.port, 15600);
    }

    #[test]
    fn parse_bare_host_without_port_uses_rpc_default() {
        let ep = Endpoint::parse("db.internal").unwrap();
        assert_eq!(ep.scheme, Scheme::Rpc);
        assert_eq!(ep.port, RPC_DEFAULT_PORT);
    }

    #[test]
    fn parse_ipv6_literal_with_port() {
        let ep = Endpoint::parse("nexus://[::1]:15475").unwrap();
        assert_eq!(ep.scheme, Scheme::Rpc);
        assert_eq!(ep.host, "::1");
        assert_eq!(ep.port, 15475);
    }

    #[test]
    fn parse_rejects_nexus_rpc_scheme() {
        // Alignment with the URL-scheme decision: `nexus-rpc://` is NOT
        // a recognised scheme — the canonical single-token form is
        // `nexus://`.
        let err = Endpoint::parse("nexus-rpc://host").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unsupported URL scheme"));
        assert!(msg.contains("nexus-rpc"));
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(Endpoint::parse("").is_err());
        assert!(Endpoint::parse("   ").is_err());
    }

    #[test]
    fn parse_rejects_invalid_port() {
        let err = Endpoint::parse("nexus://host:99999").unwrap_err();
        assert!(err.to_string().contains("invalid port"));
    }

    #[test]
    fn parse_trims_trailing_slash() {
        let ep = Endpoint::parse("nexus://host:15475/").unwrap();
        assert_eq!(ep.authority(), "host:15475");
    }

    #[test]
    fn authority_roundtrips_through_display() {
        let ep = Endpoint::parse("nexus://host:15475").unwrap();
        assert_eq!(ep.authority(), "host:15475");
        assert_eq!(ep.to_string(), "nexus://host:15475");
    }

    #[test]
    fn as_http_url_swaps_rpc_to_sibling_http_port() {
        // nexus://host:17000 -> http://host:15474 (the CLI's
        // HTTP-fallback convention assumes the sibling HTTP port is
        // the default 15474, not the input port).
        let ep = Endpoint::parse("nexus://host:17000").unwrap();
        assert_eq!(ep.as_http_url(), "http://host:15474");
    }

    #[test]
    fn as_http_url_passes_http_through_unchanged() {
        let ep = Endpoint::parse("http://host:15474").unwrap();
        assert_eq!(ep.as_http_url(), "http://host:15474");
    }

    #[test]
    fn is_rpc_matches_scheme() {
        assert!(Endpoint::parse("nexus://host").unwrap().is_rpc());
        assert!(!Endpoint::parse("http://host").unwrap().is_rpc());
        assert!(!Endpoint::parse("https://host").unwrap().is_rpc());
        assert!(Endpoint::parse("host").unwrap().is_rpc());
    }
}
