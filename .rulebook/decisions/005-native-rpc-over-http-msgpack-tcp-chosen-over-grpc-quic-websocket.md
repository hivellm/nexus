# 5. Native RPC over HTTP — msgpack + TCP chosen over gRPC / QUIC / WebSocket

**Status**: proposed
**Date**: 2026-04-19
**Related Tasks**: phase2_nexus-rpc-binary-protocol, phase2_sdk-rpc-transport-default, phase3_rpc-protocol-docs-benchmarks

## Context

Nexus ships with an HTTP/JSON endpoint on port 15474 that dominated client-SDK traffic through 0.12. For read-heavy RAG and recommendation workloads, p99 HTTP request latency was dominated by JSON parsing + per-request TCP overhead rather than actual graph traversal. phase2_nexus-rpc-binary-protocol needed to pick a new wire format for a second transport that would scale to thousands of QPS per connection without throwing away the simplicity of the existing server-side dispatcher.

## Decision

Adopt a **minimal length-prefixed MessagePack protocol over raw TCP** on port 15475. Frames: `[u32 LE length][rmp-serde body]`. Two top-level wire types (`Request`, `Response`), a single tagged-union value type (`NexusValue`), monotonic per-connection u32 request ids with `u32::MAX` reserved for server push. No HTTP layer, no HTTP/2, no TLS in v1 (terminate at LB/sidecar).

## Alternatives Considered

- gRPC — would force protobuf schemas + code generation for every SDK, pulling tokio/h2/tower into the hot path and adding a compile step to every language's SDK build. The dispatcher we already had was built around dynamic NexusValue maps; gRPC would require either typed .proto mirrors of every Cypher shape (brittle) or a generic Any type (defeats the purpose).
- QUIC / HTTP/3 — not yet stable in the Rust ecosystem circa 1.0.0; operationally harder for the single-tenant deployments most early users run.
- WebSocket — carries an HTTP upgrade handshake per connection; for a persistent socket that's not a deal-breaker but adds framing overhead (masking, text/binary ops) the workload doesn't need.
- Bolt / native Neo4j protocol — the obvious analogue, but re-implementing Bolt risks maintenance surface without a compelling compatibility story (Neo4j drivers assume Neo4j features we don't ship).
- Custom binary schema — rejected. rmp-serde's externally-tagged representation gives us a wire format that's debuggable with standard MessagePack tools, and the Synap project already proved the pattern at production scale.

## Consequences

Pros: ~2–10x lower latency and 40–60% smaller payloads on typical workloads. SDK code stays tiny — a TCP stream, an msgpack serializer, and a pending-id map. Every SDK implementation is under 1kLOC. Tooling that speaks msgpack (Grafana tails, packet captures) keeps working. No breaking change for existing HTTP callers — both transports run side-by-side.

Cons: No native TLS in v1; TLS is pushed to LB / sidecar patterns or the HTTP endpoint. No language-agnostic IDL — cross-SDK parity is enforced by mirror spec + copy-per-language command map, not by code generation. The `NexusValue` tagged union is specific to Nexus — tooling that consumes gRPC Protos or OpenAPI won't auto-understand it.

Documented in `docs/specs/rpc-wire-format.md` and `docs/OPERATING_RPC.md`.
