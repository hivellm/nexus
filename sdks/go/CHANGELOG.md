# Changelog — Nexus Go SDK

All notable changes to the Go SDK are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html) via Git tags
(Go modules read `v1.0.0` and up from the repo tag).

## [1.0.0] — 2026-04-19

### Changed

- **Version aligned to 1.0.0** across all Nexus SDKs. Tag the
  repository at `v1.0.0` to publish. No runtime behaviour changes
  in this release — the SDK continues to talk HTTP/JSON against the
  Nexus REST endpoint on port 15474.

### Pending (tracked by `phase2_sdk-rpc-transport-default` §5)

The following work lands in a subsequent 1.x release:

- **Native binary RPC transport** (`nexus://host:15475`) using
  `github.com/vmihailenco/msgpack/v5` — default transport in the
  shared SDK contract, already shipped by the Rust SDK.
- `TransportMode` enum with `"nexus"` / `"resp3"` / `"http"` string
  values (single-token, aligned with the URL scheme).
- `NEXUS_SDK_TRANSPORT` env var override.
- 500 ms connect-timeout auto-downgrade to HTTP.
- Command-map parity with the spec's §6 table.

The shared contract lives at
[`docs/specs/sdk-transport.md`](../../docs/specs/sdk-transport.md)
and the Rust SDK is the reference implementation.

## Earlier versions

Prior to 2026-04-19 the SDK tracked the server's 0.x line informally
via `go.mod` without explicit version tags.
