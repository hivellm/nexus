# nexus-protocol

Wire-protocol clients for talking to a running Nexus server.
Pure-client crate — no engine, no storage, no Cypher executor. If
you want to embed the engine in-process, depend on
[`nexus-core`](../nexus-core) instead.

## What ships

| Module | Transport | Use it for |
|---|---|---|
| `rpc` | length-prefixed MessagePack over TCP | Native binary protocol shared with the Rust SDK and `nexus-bench`. Lowest overhead. |
| `rest` | HTTP / JSON via `reqwest` | Default integration. Same shape as `curl http://127.0.0.1:15474/cypher`. |
| `mcp` | Model Context Protocol via `rmcp` | LLM tool-calling integration (Claude, etc.). |
| `umicp` | Universal Model Interoperability Protocol | Cross-provider model-tool interop. |
| `resp3` | RESP3 codec (parser + writer) | Building blocks for Redis-style framed protocols on top of TCP. |

The crate re-exports the three high-level clients at the root:

```rust
use nexus_protocol::{RestClient, McpClient, UmicpClient};
```

`rpc` is namespaced (`nexus_protocol::rpc::*`) because it ships
both a codec (`codec.rs`) and a typed message layer (`types.rs`)
that callers compose at different levels.

## Build & test

```bash
cargo +nightly build -p nexus-protocol
cargo +nightly test  -p nexus-protocol

cargo +nightly clippy -p nexus-protocol --all-targets --all-features -- -D warnings
cargo +nightly fmt --all
```

There is no `slow-tests` / `live-bench` feature here — every test
in this crate is offline and runs in milliseconds. Live transport
testing happens in [`nexus-bench`](../nexus-bench) (which depends
on this crate's `rpc` module).

## REST quick-start

```rust
use nexus_protocol::RestClient;

let client = RestClient::new("http://127.0.0.1:15474")?;
let result = client
    .execute_cypher("MATCH (n:Person) RETURN n.name LIMIT 10", None)
    .await?;

println!("{:?}", result.columns);   // ["n.name"]
println!("{:?}", result.rows);      // [["Alice"], ["Bob"], ...]
```

The response shape is **always** the Neo4j-compatible array form
(`rows: [[v1, v2]]`). No object-of-columns fallback. SDK helpers
(`RowsAsMap()` in Go / C#) are downstream conveniences, not
server-side variants.

## RPC quick-start

```rust
use nexus_protocol::rpc::{NexusRpcClient, NexusRpcCredentials};

let mut client = NexusRpcClient::connect("127.0.0.1:15475", None).await?;
client.ping().await?;
let result = client
    .execute("MATCH (n) RETURN count(n)", Default::default())
    .await?;
```

Authentication is opt-in. Pass
`Some(NexusRpcCredentials::ApiKey("..."))` or
`Some(NexusRpcCredentials::UserPassword { user, password })` to
`connect`; without credentials the handshake skips `AUTH` and
relies on the server permitting unauthenticated access.

Hard timeouts (also enforced on the server side):

| Step | Timeout |
|---|---|
| TCP connect | 5 s |
| HELLO / AUTH / PING | 2 s each |

## MCP quick-start

```rust
use nexus_protocol::McpClient;

let mut client = McpClient::connect("http://127.0.0.1:15474/mcp").await?;
let tools = client.list_tools().await?;
let result = client
    .call_tool("execute_cypher",
               serde_json::json!({ "query": "MATCH (n) RETURN n LIMIT 1" }))
    .await?;
```

## Hard constraints

- **Client-only.** No `nexus-core` dependency, no engine
  instantiation. If you find yourself reaching for storage or
  executor types, you are in the wrong crate.
- **Errors via `thiserror`.** Each transport exposes its own typed
  error (`RestClientError`, `McpClientError`, `UmicpClientError`,
  `rpc::RpcError`). Callers decide whether to bubble them up with
  `?` or wrap with `anyhow` at the application layer.
- **No `unwrap()` / `expect()`** outside of obvious compile-time
  invariants. This crate is a library; callers must be able to
  surface failures.
- **Wire format is server-defined.** Do not invent new row shapes
  here — match the server contract verbatim. See
  [`docs/specs/api-protocols.md`](../../docs/specs/api-protocols.md).

## Links

- API protocols spec: [`docs/specs/api-protocols.md`](../../docs/specs/api-protocols.md)
- Server (transport host): [`crates/nexus-server`](../nexus-server)
- Benchmark harness using `rpc`: [`crates/nexus-bench`](../nexus-bench)
- Rust SDK (also using `rpc`): [`sdks/rust`](../../sdks/rust)
