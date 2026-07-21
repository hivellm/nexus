# 4. SDK transport default is NexusRpc

**Status**: proposed
**Date**: 2026-04-19
**Related Tasks**: phase2_sdk-rpc-transport-default, phase2_cli-default-rpc-transport, phase1_nexus-rpc-binary-protocol, phase1_nexus-resp3-compatibility

## Context

Nexus ships 7 SDKs (Rust, Python, TypeScript, Go, C#, n8n, PHP) that historically all talked JSON-over-HTTP against `POST /cypher`. The server landed two faster transports in prior phases — native binary RPC (length-prefixed MessagePack on port 15475) and RESP3 (port 15476) — but the SDKs continued to default to HTTP, so users did not see the ~3–10× latency and 40–60% payload-size wins without explicit configuration. We also had inconsistent URL/scheme/env-var naming across the CLI (which chose `nexus://` as the canonical scheme) and the SDK proposal (which initially used `nexus-rpc` as the TransportMode string value).

## Decision

1. Every SDK ships RPC as the default transport. The `TransportMode` enum variants serialise to single-token strings aligned with the CLI's URL scheme: `"nexus"` (RPC, default), `"resp3"`, `"http"`. There is no `"nexus-rpc"` or `"nexus+rpc"` token anywhere in the public API.

2. URL scheme is a stronger signal than the `transport` config field: `nexus://host:port` forces RPC regardless of config, `http://host:port` forces HTTP. Bare `host:port` defaults to RPC.

3. The `NEXUS_SDK_TRANSPORT` env var overrides `ClientConfig.transport` but NOT the URL scheme. Accepted values case-insensitively: `nexus` / `rpc` / `nexusrpc`, `resp3`, `http` / `https`, `auto`.

4. Auto-downgrade to HTTP on 500 ms connect-timeout is opt-in per SDK. The Rust SDK does not auto-downgrade (RPC-only inside the SDK); Python / TypeScript / Go / C# do.

5. Admin-level Cypher (SHOW USERS, CREATE USER, CREATE DATABASE, SHOW API KEYS, TERMINATE QUERY, etc.) routes through the `CYPHER` RPC verb — the server dispatches admin clauses to the same REST handlers so SDKs need no dedicated verb per admin operation.

6. Per-SDK command-map coverage floors are release gates: Rust 100%, Python/TypeScript 95%, Go/C# 90%, n8n 85%, PHP 80%.

7. KNN embeddings travel as `NexusValue::Bytes` (little-endian f32), not `Array<Float>`, to avoid the 4× payload bloat.

The canonical specification lives in `docs/specs/sdk-transport.md` and is enforced by `phase2_sdk-rpc-transport-default`.

## Alternatives Considered

- Keep HTTP as the default, offer RPC as opt-in — rejected because nobody would turn it on and the latency win would not show up in real-world workloads.
- Ship only RPC, delete the HTTP client entirely — rejected because existing deployments need an escape hatch for networks where only 80/443 is reachable.
- Use `nexus+rpc://` or `nexus-rpc://` as the URL scheme — rejected because it disagreed with the CLI's already-shipped `nexus://` scheme and created two tokens for the same concept.
- Maintain a shared `@nexus/transport` cross-language package — rejected because the wire format will evolve and copy-per-language is the right trade-off until v1 stabilises.

## Consequences

Positive: (a) users get 3–10× lower latency and 40–60% smaller payloads without code changes; (b) SDK code is consistent across languages because the command-map, URL grammar, env-var semantics, and error classification are shared; (c) the CLI and SDKs now share one URL scheme (`nexus://`) so docs stop disagreeing with themselves; (d) RPC coverage becomes measurable per SDK because the floor is a release gate.

Negative: (a) every SDK release that adopts RPC is behaviourally default-different — users on restrictive firewalls MAY see connection failures until they opt out via `NEXUS_SDK_TRANSPORT=http` or the auto-downgrade kicks in; (b) Rust SDK becomes RPC-only internally, so operators with HTTP-only servers MUST choose a different SDK or explicitly instantiate a separate `reqwest` path — this is intentional to keep the RPC perf characteristics deterministic; (c) TypeScript browser builds cannot open raw TCP, so they're HTTP-only and do not hit the 3–10× win; (d) every SDK release needs a CHANGELOG entry calling out the default change — mitigable but not negotiable.

Mitigation: (a) one-line env-var opt-out (`NEXUS_SDK_TRANSPORT=http`); (b) `ClientConfig.transport` field; (c) auto-downgrade for SDKs that want it; (d) comprehensive per-SDK tests run the full test matrix on each of the three transports to catch regressions.
