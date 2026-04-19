# Proposal: phase2_sdk-rpc-transport-default

## Why

Nexus ships 7 SDKs today (Rust, Python, TypeScript, Go, C#, n8n, PHP) and
every one of them talks JSON-over-HTTP against `POST /cypher`. The server
changes in `phase1_nexus-rpc-binary-protocol` and
`phase1_nexus-resp3-compatibility` add two faster, cheaper transports but
deliver *zero* value until the SDKs actually use them.

Three concrete gains land the moment every SDK defaults to RPC:

1. **Out-of-box performance** — users get 3–10x lower latency and 40–60%
   smaller payloads without changing a single line of their application
   code. Synap did this in their v2 and saw a measurable p99 latency drop
   across all seven SDKs.
2. **One command-map, many SDKs** — Synap's TypeScript SDK pioneered a
   `mapCommand(cmd, payload) -> {rawCmd, args}` layer that translates the
   SDK's dotted names (`graph.cypher`, `node.create`, `knn.search`) into
   wire commands. We reuse that pattern so all SDKs share one
   source-of-truth table for command mappings.
3. **Transport-agnostic managers** — every manager (QueryBuilder, Schema,
   Batch, Transaction) keeps the same JSON-shaped API that users already
   know. The transport layer underneath just picks RPC/RESP3/HTTP based on
   a config flag. Failing back to HTTP for not-yet-mapped commands means we
   can ship the upgrade **incrementally** rather than blocking on 100%
   coverage.

We **set RPC as the default** because it is measurably the best transport
on every axis (latency, throughput, payload size) *except* tooling
accessibility — and tooling accessibility is the job of the RESP3 port, not
the SDK. Users with exotic network policies can opt down to RESP3 or HTTP
with a one-line config change.

## What Changes

Every SDK gains the same three-file shape Synap uses, adapted to Nexus
commands:

```
sdks/<lang>/src/transports/
|- rpc.ts               # binary RPC implementation
|- resp3.ts
|- command-map.ts       # maps SDK dotted names to wire commands
|- index.ts             # TransportMode enum + factory
```

Public API addition: every SDK's `ClientConfig` grows a single field.
**Canonical string values for TransportMode stay single-token** so they
line up with the `nexus://` URL scheme used by the CLI and with the
`NEXUS_SDK_TRANSPORT` env var:

```ts
enum TransportMode {
  NexusRpc = 'nexus',       // default — Nexus native binary RPC
  Resp3    = 'resp3',
  Http     = 'http',
}

interface NexusConfig {
  baseUrl:   string;                 // used for HTTP fallback
  transport?: TransportMode;         // default NexusRpc
  rpcPort?:   number;                // default 15475
  resp3Port?: number;                // default 15476
  ...
}
```

URLs passed to `ClientConfig.baseUrl` may use `nexus://host:15475` to
force the RPC transport regardless of the `transport` field (the
factory honours the URL scheme as a stronger signal than the config
hint). There is no `nexus-rpc://` scheme — the token is `nexus`.

All existing manager methods (`client.executeCypher()`,
`client.schema.listLabels()`, `client.batch.ingest()`, …) keep their exact
signatures. Internally they call `transport.execute(cmd, payload)` which
either maps to RPC/RESP3 or falls back to HTTP for unmapped commands.

Command-map coverage targets (measured % of SDK manager methods covered
by native wire mapping, Phase 2 exit criteria):

| SDK        | Mapping coverage target | Fallback           |
|------------|-------------------------|--------------------|
| Rust       | 100%                    | HTTP not shipped   |
| Python     | 95%                     | HTTP REST client   |
| TypeScript | 95%                     | HTTP REST client   |
| Go         | 90%                     | HTTP REST client   |
| C#         | 90%                     | HTTP REST client   |
| n8n        | 85%                     | HTTP (n8n native)  |
| PHP        | 80%                     | HTTP REST client   |

Rust ships with RPC-only (no HTTP fallback inside the SDK) because it's
the performance-sensitive SDK and we want to eliminate codepaths. PHP has
the lowest target because the RESP3 interop via `predis` is easier to
maintain than RPC for that ecosystem.

Across SDKs, the wire-level `NexusValue` <-> language-value conversion
follows Synap's `toWireValue` / `fromWireValue` with language-idiomatic
adapters:

| Lang       | Library                       | Bytes handling          |
|------------|-------------------------------|-------------------------|
| Rust       | `rmp-serde`                   | `Vec<u8>` native        |
| Python     | `msgpack` (pure-python)       | `bytes` native          |
| TypeScript | `msgpackr`                    | `Uint8Array` native     |
| Go         | `github.com/vmihailenco/msgpack/v5` | `[]byte`          |
| C#         | `MessagePack-CSharp`          | `byte[]`                |
| n8n        | shares TS SDK                 | `Buffer`                |
| PHP        | `rybakit/msgpack.php`         | native bytes            |

Persistent connections, per-request `id`, automatic reconnect, and a
dedicated subscription connection (for future Cypher streaming) are all
copied from Synap's transport — the logic is already field-tested.

## Impact

- **Affected specs**: `/docs/specs/sdk-transport.md` (NEW), plus a bullet
  in each SDK's README under "Quick Start".
- **Affected code**:
  - Rust SDK: +`sdks/rust/src/transport/`, MODIFIED `client.rs` to route
    via transport
  - Python SDK: +`synap_sdk/transport_rpc.py`, `transport_resp3.py`,
    `command_map.py`, MODIFIED `client.py`
  - TypeScript SDK: +`transports/` directory, `command-map.ts`, MODIFIED
    `client.ts`
  - Go SDK: +`transport_rpc.go`, `transport_resp3.go`, `command_map.go`,
    MODIFIED `client.go`
  - C# SDK: +`Transports/` namespace, MODIFIED `NexusClient.cs`
  - n8n: shared with TS SDK; reuse via direct dependency
  - PHP: +`Transport/` namespace using `predis` for RESP3 and a
    hand-written RPC client
- **Breaking change**: NO for existing users (default HTTP behavior is
  preserved with an env var `NEXUS_SDK_TRANSPORT=http`). YES for the
  *default* behavior — new SDK installs will use RPC by default. Mitigated
  by: (a) a one-line env-var opt-out, (b) automatic fallback to HTTP if
  the RPC port is unreachable within a 500 ms connect timeout, (c) a
  changelog entry per SDK calling this out explicitly.
- **User benefit**: 3–10x lower query latency, 40–60% smaller payloads,
  drop-in: zero code changes required to adopt.

## Non-goals

- Changing public manager signatures (CRUD, query builders, transactions).
- Streaming Cypher or live-query subscriptions (deferred to V2; the push
  channel infrastructure is wired in the RPC server from day one but no
  SDK consumer is shipped in Phase 2).
- A shared `@nexus/transport` cross-SDK package. Copy-per-language is the
  right trade-off until the protocol stabilises.

## Reference

Synap SDK transport implementations (direct analogues):

- Rust: `sdks/rust/src/transport/mod.rs` (930 LOC, RPC + RESP3 + HTTP fallback)
- Python: `sdks/python/synap_sdk/transport_rpc.py`, `transport_resp3.py`
- TypeScript: `sdks/typescript/src/transports/{synap-rpc,resp3,command-map}.ts`
- Go: `sdks/go/` transport layer
- C#: `sdks/csharp/` transport layer
