# 2. SDK default transport is Nexus RPC with auto-downgrade to HTTP

**Status**: proposed
**Date**: 2026-04-18
**Related Tasks**: phase2_sdk-rpc-transport-default, phase3_rpc-protocol-docs-benchmarks

## Context

Nexus ships 7 SDKs (Rust, Python, TypeScript, Go, C#, n8n, PHP). After v1.0.0 lands the binary RPC and RESP3 server transports, every SDK must pick a default. Three concerns: (1) existing v0.12 users should not break on upgrade, (2) users stuck behind firewalls that block port 15475 need a deterministic fallback, (3) adding a new SDK should not require rewriting the command-map table per language.

## Decision

All SDKs default to Nexus RPC transport with the following behavior:

- `TransportMode` enum: `NexusRpc` (default), `Resp3`, `Http`.
- `ClientConfig.transport` is optional; omitted means `NexusRpc`.
- Env var `NEXUS_SDK_TRANSPORT=http|resp3|nexus-rpc` overrides per-process (deployment-level opt-out).
- Auto-downgrade chain on connect failure: RPC (15475) → HTTP (15474) with a 500 ms connect timeout. RESP3 is opt-in only, never auto-selected.
- Not-yet-mapped commands fall back to HTTP transparently inside the SDK (Rust SDK is the exception: RPC-only, no HTTP fallback inside the SDK).
- Command-map coverage targets (exit criteria for phase 2): Rust 100%, Python/TS 95%, Go/C# 90%, n8n 85%, PHP 80%.
- The command-map is versioned in `docs/specs/sdk-transport.md` as the single source of truth; every SDK implements the same table.
- Manager method signatures (QueryBuilder, Schema, Batch, Transaction) stay identical to v0.12 so users have zero code changes.

Rollout: ship v1.0.0 with the default change in a minor SDK version per language; CHANGELOG per SDK explicitly calls out the default switch and the one-line opt-out.

## Alternatives Considered

- Keep HTTP as default, RPC opt-in: users would not benefit without explicit action, and 'faster by default' is the whole point of the v1.0.0 release. Rejected.
- RPC-only, no HTTP fallback in SDKs: cleanest codepath but brittle for users behind restrictive firewalls or debugging with curl. Kept only for Rust SDK where performance is the top priority.
- Negotiate transport over HTTP /version endpoint before first RPC call: adds round-trip on every startup, defeats the latency goal. Rejected; static defaults + env override is simpler.
- Per-SDK decision: each SDK picks its own default based on ecosystem norms (e.g. PHP stays HTTP). Rejected because it fragments the documentation story and confuses users moving between languages.

## Consequences

Positive:
- New installs get 3-10x faster queries automatically, no user action required.
- Existing users with restrictive firewalls get a deterministic fallback path (auto-downgrade or env override).
- Single shared command-map table forces cross-SDK parity and catches divergence early.
- Documentation has one transport-selection flowchart, not seven.

Negative:
- Users who upgrade SDK major version without opening port 15475 will observe a one-time 500 ms startup delay before the auto-downgrade kicks in. Mitigated by prominent CHANGELOG note + DEBUG-level log on downgrade.
- Every SDK now has to maintain three transport implementations (RPC, RESP3, HTTP). Mitigated by the thin command-map layer that concentrates transport-specific code in ~400 LOC per SDK.
- Rust SDK diverges (no HTTP fallback) - creates a small asymmetry. Acceptable because the Rust SDK is the reference implementation and its users are performance-sensitive by definition.
