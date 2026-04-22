//! Transport layer for the Rust SDK.
//!
//! Every `NexusClient` sits on top of a [`Transport`]. Three transport
//! modes are recognised:
//!
//! - `NexusRpc` — native binary RPC (length-prefixed MessagePack on
//!   port 15475). **Default.**
//! - `Http` / `Https` — legacy JSON over REST on port 15474 / 443.
//! - `Resp3` — reserved for future RESP3 support (not yet shipped in
//!   the SDK; the enum variant exists so `TransportMode::from_str`
//!   accepts the token).
//!
//! Configuration precedence:
//!
//! 1. **URL scheme** in `ClientConfig.base_url` — `nexus://` forces
//!    RPC, `http://` / `https://` forces HTTP. Strongest signal.
//! 2. **Env var** `NEXUS_SDK_TRANSPORT` — overrides the config
//!    field but NOT the URL scheme.
//! 3. **`ClientConfig.transport`** field — explicit hint when the URL
//!    is bare (`host:port`).
//! 4. **Default**: `TransportMode::NexusRpc`.
//!
//! See `docs/specs/sdk-transport.md` for the canonical contract that
//! every SDK (Rust, Python, TypeScript, Go, C#, n8n, PHP) implements.

pub mod command_map;
pub mod endpoint;
pub mod http;
pub mod rpc;

pub use command_map::{CommandMapping, map_command};
pub use endpoint::{Endpoint, Scheme};

use crate::error::Result;
use async_trait::async_trait;
use nexus_protocol::rpc::types::NexusValue;

/// Which wire transport the client uses.
///
/// Aligned with the CLI's URL-scheme tokens and with the
/// `NEXUS_SDK_TRANSPORT` env var values (`"nexus"`, `"resp3"`,
/// `"http"`). The Rust variant names stay in PascalCase for
/// idiomatic Rust; the wire-level representation is the lowercase
/// single-token string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    /// Native binary RPC — length-prefixed MessagePack on port 15475.
    NexusRpc,
    /// RESP3 on port 15476. Reserved; not yet shipped in the SDK.
    Resp3,
    /// HTTP/JSON on port 15474 (or 443 for https).
    Http,
    /// HTTPS/JSON on port 443.
    Https,
}

impl TransportMode {
    /// Parse the `NEXUS_SDK_TRANSPORT` env var token or the string
    /// form a caller might stash in a config file. Accepts the
    /// canonical values from `docs/specs/sdk-transport.md` plus the
    /// `"rpc"` / `"nexusrpc"` aliases for ergonomics.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "nexus" | "rpc" | "nexusrpc" => Some(Self::NexusRpc),
            "resp3" => Some(Self::Resp3),
            "http" => Some(Self::Http),
            "https" => Some(Self::Https),
            "" | "auto" => None,
            _ => None,
        }
    }

    /// True if the mode carries the native binary RPC wire format.
    pub fn is_rpc(self) -> bool {
        matches!(self, Self::NexusRpc)
    }
}

impl std::fmt::Display for TransportMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NexusRpc => "nexus",
            Self::Resp3 => "resp3",
            Self::Http => "http",
            Self::Https => "https",
        })
    }
}

/// A single-frame request against the active transport.
#[derive(Debug, Clone)]
pub struct TransportRequest {
    /// Wire-level command name (`CYPHER`, `PING`, `STATS`, ...).
    pub command: String,
    /// Positional arguments as already-encoded `NexusValue` entries.
    pub args: Vec<NexusValue>,
}

/// A single-frame response from the active transport. The
/// [`TransportMode::NexusRpc`] path carries this directly; the
/// [`TransportMode::Http`] path decodes the REST JSON response into
/// an equivalent `NexusValue::Map` envelope.
#[derive(Debug, Clone)]
pub struct TransportResponse {
    /// The decoded server reply.
    pub value: NexusValue,
}

/// Generic transport interface — one method per request/response pair.
///
/// Every concrete transport (RPC, HTTP, eventually RESP3) implements
/// this trait so `NexusClient` can remain transport-agnostic.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a single request and wait for the matching response.
    async fn execute(&self, req: TransportRequest) -> Result<TransportResponse>;

    /// Short human-readable description (e.g. `"nexus://host:15475 (RPC)"`)
    /// used by `NexusClient::endpoint_description()` for `--verbose` output.
    fn describe(&self) -> String;

    /// True when the active transport uses the native binary RPC
    /// wire format. Lets the client layer quickly skip HTTP-only
    /// fallback paths.
    fn is_rpc(&self) -> bool;
}
