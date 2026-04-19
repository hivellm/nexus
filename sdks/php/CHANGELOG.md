# Changelog — Nexus PHP SDK

All notable changes to the PHP SDK are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html) via Git tags
(Composer resolves `^1.0` from the `v1.0.0` repo tag).

## [1.0.0] — 2026-04-19

### Changed

- **Version aligned to 1.0.0** across all Nexus SDKs. Tag the
  repository at `v1.0.0` to publish on packagist. No runtime
  behaviour changes in this release — the SDK continues to talk
  HTTP/JSON against the Nexus REST endpoint on port 15474.

### Pending (tracked by `phase2_sdk-rpc-transport-default` §8)

The following work lands in a subsequent 1.x release:

- **Native binary RPC transport** (`nexus://host:15475`) using
  `rybakit/msgpack` for the MessagePack body + hand-rolled
  length-prefix framing — default transport in the shared SDK
  contract, already shipped by the Rust SDK.
- RESP3 transport via `predis/predis`.
- `Transport` interface + `TransportMode` enum with `"nexus"` /
  `"resp3"` / `"http"` string values.
- `NEXUS_SDK_TRANSPORT` env var override.
- 500 ms connect-timeout auto-downgrade to HTTP.
- Command-map parity with the spec's §6 table.

The shared contract lives at
[`docs/specs/sdk-transport.md`](../../docs/specs/sdk-transport.md)
and the Rust SDK is the reference implementation.

## Earlier versions

Prior to 2026-04-19 the SDK shipped without an explicit version
field (Composer consumers pulled `dev-main` from the mono-repo).
