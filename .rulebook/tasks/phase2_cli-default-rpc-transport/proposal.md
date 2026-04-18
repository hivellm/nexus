# Proposal: make the binary RPC transport the default for `nexus-cli`

## Why

`nexus-cli` (see [nexus-cli/src/client.rs](nexus-cli/src/client.rs))
currently talks to the server over HTTP/JSON against `/cypher`,
`/health`, `/status`, `/stats`, `/auth/*`, `/export`, `/import`. The
default base URL is `http://localhost:3000` — wrong for Nexus (the REST
port is `15474`) and unrelated to the RPC port `15475` that
`phase1_nexus-rpc-binary-protocol` shipped.

Two problems follow:

1. **CLI latency + envelope size.** Every invocation eats an HTTP
   handshake and JSON (de)serialisation even for tiny operations like
   `nexus db list` or `nexus query 'RETURN 1'`. The RPC transport uses
   length-prefixed MessagePack on a persistent TCP connection —
   measurably smaller envelopes, no header overhead, no per-request
   TCP handshake when the CLI is used in a REPL-style loop.
2. **Dogfooding gap.** The SDKs ship RPC as default, but the
   first-party CLI contradicts that by pointing at REST. RPC edge
   cases (connection loss, partial frames, push-id rejection,
   `auth.required`) stay unexercised by the interactive workflow we
   use the most. "RPC is the default transport" is project policy
   already — the CLI needs to match.

The correct posture is the one the SDKs take: **RPC on `15475` is the
default** and `--transport http` (or `NEXUS_TRANSPORT=http`) opts back
into REST for diagnostics or legacy servers.

## What Changes

- **New `nexus-cli/src/transport/` module.**
  - Add a `Transport` trait covering every operation the CLI uses
    (cypher, health, stats, list users / api keys / databases, schema
    and data ops used by `commands::{schema,data}`, export/import).
  - Two implementations:
    - `RpcTransport` — wraps `nexus_protocol::rpc` (`codec` + `types`)
      over a persistent TCP stream.
    - `HttpTransport` — the existing `reqwest`-based client, refactored
      to sit behind the trait.
  - `Transport::connect(url, config)` factory that parses
    `nexus-rpc://host:15475`, `nexus+rpc://…`, `http://…`, `https://…`
    and the shorthand `host:port` (defaulting to RPC on `15475` when
    no scheme is given).
- **Default behaviour.**
  - Without `--server` / `NEXUS_SERVER`, assume
    `nexus-rpc://127.0.0.1:15475`.
  - `--transport rpc|http` flag + `NEXUS_TRANSPORT` env var force the
    transport even if the URL scheme disagrees. `NEXUS_TRANSPORT=http`
    preserves today's behaviour for CI that cannot update immediately.
  - `--verbose` prints the resolved URL + chosen transport so users
    can diagnose which path a command took.
- **Commands.** `commands::query`, `commands::schema::*`,
  `commands::data::*`, `commands::db::*`, `commands::user::*`,
  `commands::key::*`, `commands::admin::*` route through the trait,
  not through the concrete HTTP client.
  - Commands whose RPC verb already exists (cypher, ping, db.list /
    create / drop / use, ingest, knn.* — see
    `nexus-server/src/protocol/rpc/dispatch/*`) go straight to RPC.
  - Commands without an RPC verb today (export, import, some auth
    admin ops) fall back to HTTP with an explicit `eprintln!` noting
    the fallback. No silent fallback.
- **Config + docs.**
  - `nexus-cli/src/config.rs`: drop the `http://localhost:3000`
    default; replace with an `EndpointConfig { scheme: Rpc, host:
    "127.0.0.1", port: 15475 }` struct. Precedence: CLI flag > env
    var > config file > compiled-in default.
  - `nexus-cli/README.md` — "Configuration" section updated to show
    RPC as default, with an explicit opt-out recipe.
  - `docs/` — add or extend a CLI guide with a "Transports" section
    covering the flag, env var, and how to health-check each.

## Impact
- Affected specs: none (CLI-only behaviour change, no protocol change).
- Affected code:
  - `nexus-cli/src/client.rs` (split into HTTP transport impl)
  - `nexus-cli/src/main.rs` (flag + env parsing)
  - `nexus-cli/src/config.rs` (endpoint defaults)
  - `nexus-cli/src/commands/*.rs` (route through trait)
  - `nexus-cli/Cargo.toml` (depend on `nexus-protocol`)
- Breaking change: YES — users relying on the unreachable
  `http://localhost:3000` default get a different (correct) endpoint.
  Anyone passing an explicit `--server http://...` URL is unaffected.
- User benefit: faster CLI invocations, consistent defaults with the
  SDKs, RPC surface exercised by interactive use.

## Source

- `phase1_nexus-rpc-binary-protocol` (archived) — server side + SDK
  wrapping.
- `nexus-server/src/protocol/rpc/` — reference implementation of the
  command set served on `15475`.
- `nexus-protocol/src/rpc/{codec,types}.rs` — shared wire format the
  CLI must reuse (no duplicated types).
- `sdks/rust/` — how the Rust SDK wraps RPC; the CLI transport follows
  the same shape.

## Out of Scope
- Changing *server* defaults. RPC is already listening on `15475`.
- Deprecating the REST API. It stays for diagnostic use.
- Adding new RPC commands that are not a prerequisite for an existing
  CLI subcommand. If a command has no RPC verb today, keep the HTTP
  fallback with a warning instead of expanding the protocol surface.
