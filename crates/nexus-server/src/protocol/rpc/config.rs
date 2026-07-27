//! The Nexus RPC Thunder profile — how Nexus uses the shared Thunder wire.
//!
//! This is the **server's own copy** of the wire constants. Per the
//! phase10 decision to dissolve `nexus-protocol` and NOT keep a shared
//! protocol crate, every SDK re-declares these same literals independently
//! in its own language (phase11 gives the Rust SDK its own copy). With no
//! shared type, the pin test below is the *only* thing holding the two ends
//! of the wire together — treat it as load-bearing. Mirror of Synap's
//! `crates/synap-server/src/protocol/synap_rpc/config.rs`.
//!
//! Divergences from `Config::standard()` and why:
//!
//! | dimension        | value                 | why |
//! |------------------|-----------------------|-----|
//! | `scheme`         | `nexus`               | endpoint URIs are `nexus://host:port` |
//! | `default_port`   | 15475                 | the historical Nexus RPC port |
//! | `handshake`      | `AuthCommand`         | auth via an `AUTH` command frame, not a HELLO map |
//! | `hello_style`    | `NotUsed`             | no HELLO handshake payload (matches Synap) |
//! | `push`           | `Reserved`            | no push-producing commands ship yet (unlike Synap's `Enabled`) |
//! | `error_codes`    | `Resp3Prefixes`       | `NOAUTH`/`WRONGPASS`/… tokens, shared with the RESP3 listener |
//! | `max_frame_bytes`| 64 MiB                | the current `DEFAULT_MAX_FRAME_BYTES`; `NEXUS_RPC_MAX_FRAME_BYTES` still overrides at the listener |

use thunder::Config;
use thunder::wire::config::{ErrorConvention, Handshake, HelloStyle, PushPolicy};

/// Default Nexus RPC port (historical `nexus-protocol` value).
pub const DEFAULT_RPC_PORT: u16 = 15475;

/// Frame-body cap carried over from `nexus_protocol::rpc::DEFAULT_MAX_FRAME_BYTES`.
/// The listener still lets `NEXUS_RPC_MAX_FRAME_BYTES` override the runtime cap.
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// The Nexus RPC Thunder profile. `const` so it can be pinned by a test and
/// embedded without a runtime allocation.
pub const fn nexus_thunder_config() -> Config {
    Config::standard()
        .scheme("nexus")
        .port(DEFAULT_RPC_PORT)
        .handshake(Handshake::AuthCommand)
        .hello_style(HelloStyle::NotUsed)
        .push(PushPolicy::Reserved)
        .error_codes(ErrorConvention::Resp3Prefixes)
        .max_frame_bytes(MAX_FRAME_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin every field the SDKs re-declare independently. With no shared
    /// wire type across projects, this assertion is the only guard against
    /// silent drift between the Nexus server and its clients — if any value
    /// here changes, a client speaking the old profile breaks, so a change
    /// must be deliberate and land on both ends together.
    #[test]
    fn config_matches_what_the_sdks_assume() {
        let c = nexus_thunder_config();
        assert_eq!(c.scheme, "nexus");
        assert_eq!(c.default_port, 15475);
        assert_eq!(c.handshake, Handshake::AuthCommand);
        assert_eq!(c.hello_style, HelloStyle::NotUsed);
        assert_eq!(c.push, PushPolicy::Reserved);
        assert_eq!(c.error_codes, ErrorConvention::Resp3Prefixes);
        assert_eq!(c.max_frame_bytes, 64 * 1024 * 1024);
    }

    /// The frame cap must not silently drop below the historical
    /// `nexus-protocol` value that deployed clients were built against.
    #[test]
    fn frame_cap_is_not_below_the_legacy_cap() {
        assert!(nexus_thunder_config().max_frame_bytes >= 64 * 1024 * 1024);
    }
}
