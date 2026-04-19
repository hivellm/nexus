# Migrating to the RPC-first SDK transport (1.0.0)

Every first-party Nexus SDK — Rust, Python, TypeScript, Go, C#, PHP —
**now defaults to the native binary RPC transport** on
`nexus://127.0.0.1:15475`. The previous releases (0.12.x for
TypeScript, 0.1.x for the rest) defaulted to HTTP on port `15474`.

This guide answers the two questions every existing user will have:

1. _Do I need to change my code?_ — Probably not, but read below.
2. _How do I opt out if my deployment can't open port 15475?_ — One env
   var or one kwarg.

## What changed

- **Default URL scheme** went from `http://` to `nexus://` on every
  SDK. The default port is now `15475` (binary RPC) instead of
  `15474` (HTTP/JSON).
- Every SDK's config grew three new fields: `transport`, `rpcPort`,
  `resp3Port` (or the language-idiomatic equivalent).
- A new `NEXUS_SDK_TRANSPORT` environment variable lets ops pick the
  transport without changing the code.
- The on-wire representation uses length-prefixed MessagePack frames.
  3–10x lower latency and 40–60% smaller payloads on typical graph
  workloads. See `docs/specs/sdk-transport.md` for the exact wire
  format.

## Transport precedence (identical across SDKs)

When constructing a client the SDK resolves the transport in this
order (highest wins):

1. **URL scheme** in `baseUrl` / `BaseUrl` / `base_url`.
   - `nexus://host[:port]` → binary RPC.
   - `http://host[:port]` → HTTP/JSON.
   - `https://host[:port]` → HTTPS/JSON.
   - `resp3://host[:port]` → RESP3 (not yet shipped; throws).
2. **Environment variable** `NEXUS_SDK_TRANSPORT`. Accepts
   `nexus`/`rpc`/`nexusrpc`, `resp3`, `http`, `https`.
3. **Config field** `transport`.
4. **Default**: `nexus` (binary RPC).

The URL scheme always wins over env + config so deployments can pin a
specific transport without the SDK second-guessing them.

## Will my code break?

**Scenario 1 — you passed an explicit `http://…` URL.** No changes
required; the SDK still honours the URL scheme.

**Scenario 2 — you relied on the default.** Your calls will now go
to port `15475`. Fix either:

- Set `NEXUS_SDK_TRANSPORT=http` in the environment, **or**
- Pass an explicit HTTP URL, **or**
- Start the Nexus server (1.0.0 opens `15475` by default).

**Scenario 3 — you authenticated with API key or basic auth.** Works
on both transports. RPC sends an `AUTH` frame right after the HELLO
handshake; HTTP sends `X-API-Key` / `Authorization` headers.

**Scenario 4 — you used CRUD helpers (`createNode`, `updateNode`,
etc.).** These helpers still hit REST endpoints on the sibling HTTP
port (`15474`) via a side-car HTTP client. They continue to work when
the primary transport is RPC, but their network path is unchanged.
For full RPC coverage, call `executeCypher` directly with the
equivalent `CREATE` / `MATCH` / `SET` / `DELETE` statements.

**Scenario 5 — you typed a `catch` against a specific error class.**
- Rust: `NexusError::Api { status, message }` still surfaces HTTP
  failures.
- Go: `*nexus.Error` is preserved — the transport layer translates
  `*transport.HttpError` back into `*nexus.Error` before returning.
- TypeScript / Python / C# / PHP: the SDK-level error types are
  unchanged; the transport layer produces them from either path.

## Opt-out recipes per language

### Rust

```rust
use nexus_sdk::{ClientConfig, NexusClient, TransportMode};

let client = NexusClient::with_config(ClientConfig {
    base_url: "http://localhost:15474".to_string(),
    transport: Some(TransportMode::Http),
    ..Default::default()
})?;
```

Env var: `NEXUS_SDK_TRANSPORT=http`.

### TypeScript

```ts
import { NexusClient } from '@hivehub/nexus-sdk';

const client = new NexusClient({
  baseUrl: 'http://localhost:15474',
  // or transport: 'http', or NEXUS_SDK_TRANSPORT=http
  auth: { apiKey: process.env.NEXUS_API_KEY },
});
```

### Python

```python
from nexus_sdk import NexusClient, TransportMode

async with NexusClient(
    base_url="http://localhost:15474",
    api_key=os.environ.get("NEXUS_API_KEY"),
    # or transport=TransportMode.HTTP
) as client:
    await client.execute_cypher("RETURN 1")
```

Env var: `export NEXUS_SDK_TRANSPORT=http`.

### Go

```go
import (
    nexus "github.com/hivellm/nexus-go"
    "github.com/hivellm/nexus-go/transport"
)

client := nexus.NewClient(nexus.Config{
    BaseURL:   "http://localhost:15474",
    APIKey:    os.Getenv("NEXUS_API_KEY"),
    Transport: transport.ModeHttp,
})
defer client.Close()
```

### C#

```csharp
using Nexus.SDK;
using Nexus.SDK.Transports;

await using var client = new NexusClient(new NexusClientConfig
{
    BaseUrl = "http://localhost:15474",
    ApiKey = Environment.GetEnvironmentVariable("NEXUS_API_KEY"),
    // or Transport = TransportMode.Http
});
```

### PHP

```php
use Nexus\SDK\Config;
use Nexus\SDK\NexusClient;
use Nexus\SDK\Transport\TransportMode;

$client = new NexusClient(new Config(
    baseUrl: 'http://localhost:15474',
    apiKey: getenv('NEXUS_API_KEY') ?: null,
    transport: TransportMode::Http,
));
```

## Firewall considerations

- **Port 15475** carries raw binary TCP (length-prefixed MessagePack).
  If you route SDK traffic through HTTP-aware load balancers or
  reverse proxies, they will not handle RPC correctly — keep those on
  port `15474` (HTTP) and point the SDK at the load balancer with
  `http://…`.
- **TLS** on the RPC port is not yet in the shipped 1.0.0 SDK. For
  TLS deployments use `https://host:443`. The RESP3 debug port
  (`15476`) remains plain-text and is meant for diagnostic tools.

## Rolling out to production

1. Deploy Nexus server 1.0.0 — the RPC listener is on by default.
2. Ship SDK 1.0.0 to callers. If you want a cautious rollout, set
   `NEXUS_SDK_TRANSPORT=http` globally first so nothing moves onto
   RPC until you unset it per tier.
3. Once every caller is on SDK 1.0.0 and you've verified RPC works
   end-to-end, unset the env var (or switch it to `nexus`) to let the
   default take over.
4. The HTTP endpoint stays on forever — it's the blessed path for
   browser builds, third-party tools, and ops scripts.

## Spec + source of truth

- Cross-SDK contract: [`docs/specs/sdk-transport.md`](specs/sdk-transport.md).
- Wire format: [`docs/specs/rpc-wire-format.md`](specs/rpc-wire-format.md).
- Reference implementation: Rust SDK (`sdks/rust/src/transport/`).
- Task history: [`.rulebook/tasks/phase2_sdk-rpc-transport-default/`](../.rulebook/tasks/phase2_sdk-rpc-transport-default/).

## Questions

File an issue on the Nexus repository with `[sdk-transport]` in the
title or ping the team at `team@hivellm.org`.
