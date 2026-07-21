# 1. Native RPC wire format: 4-byte LE length prefix + MessagePack body

**Status**: proposed
**Date**: 2026-04-18
**Related Tasks**: phase1_nexus-rpc-binary-protocol, phase2_sdk-rpc-transport-default, phase3_rpc-protocol-docs-benchmarks

## Context

Nexus v1.0.0 needs a transport faster than JSON/HTTP for SDK traffic. Synap already validated a native binary RPC with 4-byte LE length prefix + rmp-serde MessagePack body, per-connection writer task, per-request Tokio task, and reserved `id = u32::MAX` for server-initiated push frames. Without locking the wire format up-front, every SDK would have to be rewritten the moment the framing or reserved-id semantics change.

## Decision

Nexus RPC wire format mirrors Synap's SynapRPC exactly, renamed:

- Frame: `[u32 LE length][rmp-serde body]`, max 64 MiB per frame.
- Request: `struct Request { id: u32, command: String, args: Vec<NexusValue> }`.
- Response: `struct Response { id: u32, result: Result<NexusValue, String> }`.
- `NexusValue` is an externally-tagged enum: Null, Bool, Int(i64), Float(f64), Bytes(Vec<u8>), Str(String), Array(Vec<NexusValue>), Map(Vec<(NexusValue, NexusValue)>).
- `id` is caller-chosen; echoed on the matching Response to support out-of-order multiplexing.
- `id = u32::MAX` is reserved exclusively for server-initiated push frames (Cypher streaming, index change notifications in V2).
- Default TCP port: 15475 (REST is 15474). RESP3 compat port is 15476.
- Error prefixes in `Response::err`: `ERR`, `WRONGTYPE`, `NOAUTH`, `TIMEOUT`, `RATE_LIMIT`, `NOTFOUND` (echoes Redis/Synap convention).
- Authentication: `HELLO` + `AUTH` commands. No TLS in V1; terminate at LB or stunnel.

This format is frozen for v1.0.0. Any change after v1.0.0 requires a new wire-format-version field and coordinated SDK release.

## Alternatives Considered

- gRPC with protobuf: 40-60% more CPU per message than msgpack for small payloads, requires code generation step, adds grpc-rs and prost to the dep tree. Rejected because the project values minimal deps and the ecosystem already has rmp-serde.
- QUIC with custom framing: modern, solves head-of-line blocking, but no production-grade Rust server crate at our stability bar in 2026, and TCP is sufficient for localhost/LAN latency targets. Deferred to V2 if clustering demands it.
- rkyv zero-copy framing: 2-3x faster decode than msgpack, but produces self-referential layouts that are hard to interoperate across SDKs (rkyv has no idiomatic Python/Go decoder). Rejected on cross-language portability.
- WebSocket + JSON: easier for browser, but WebSocket framing is more expensive than raw TCP+length-prefix and we already ship RESP3 for cURL-class tooling. Rejected as redundant.

## Consequences

Positive:
- Seven SDKs share one canonical wire format already proven in Synap.
- msgpack is 3-10x faster than JSON and 40-60% smaller on Cypher result payloads.
- `id=u32::MAX` sentinel keeps push support on the same TCP connection with zero protocol churn.
- New TCP port (15475) is independent of existing HTTP/MCP endpoints; no migration for existing REST users.

Negative:
- Operators must open an additional TCP port; firewall-restricted deployments need config change (`rpc.enabled = false`).
- 64 MiB frame cap means ingest batches above that size must chunk.
- MessagePack numerics lose the "integer vs float" distinction at high magnitudes (outside i64 range). Mitigated by documenting that graph ids fit in i64 and KNN scores are f64.
- SDK authors adding a new language must implement msgpack framing; acceptable cost given language support in all major ecosystems.
