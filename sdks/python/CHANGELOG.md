# Changelog — Nexus Python SDK

All notable changes to the Python SDK are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [1.0.0] — 2026-04-19

### Changed

- **Version aligned to 1.0.0** across all Nexus SDKs (was `0.1.0`).
  No runtime behaviour changes in this release — the SDK continues to
  talk HTTP/JSON against the Nexus REST endpoint on port `15474`.

### Pending (tracked by `phase2_sdk-rpc-transport-default` §4)

The following work lands in a subsequent 1.x release:

- Native binary RPC transport (`nexus://host:15475`) — default
  transport in the shared SDK contract, already shipped by the Rust
  SDK. Python implementation will use `asyncio` + `msgpack`.
- `NEXUS_SDK_TRANSPORT` env var + `ClientConfig.transport` override.
- RESP3 transport.
- 500 ms connect-timeout auto-downgrade to HTTP.
- Command-map parity with the spec's §6 table.

The shared contract lives at [`docs/specs/sdk-transport.md`](../../docs/specs/sdk-transport.md)
and the Rust SDK is the reference implementation.

## Earlier versions

The SDK predated formal changelog tracking. See git history prior to
2026-04-19 for the 0.1.0 implementation notes.
