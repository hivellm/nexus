# Proposal: phase11_thunder-client-migration

## Why

After phase10 the server speaks Thunder natively. Every Nexus client still carries its
own hand-written copy of the same wire: the CLI (`crates/nexus-cli/src/rpc_transport.rs`)
and all 6 SDKs, each with a per-language MessagePack codec + RPC transport
(`rpc` + `codec` modules, ~400–600 LOC per language). Thunder ships maintained native
client packages for every one of those languages (`thunder-rpc`, `@hivehub/thunder`,
`hivellm-thunder`, `thunder-go`, `HiveLLM.Thunder`, `hivellm/thunder`), each with the
uniform client floor the hand-rolled transports lack: id demux/pipelining over one
connection, frame-cap validation **before** allocation (Synap found 9 of 15 family SDK
transports allocated from an untrusted length prefix), connect + per-call timeouts, lazy
reconnect with capped retries, typed error classes, and a push hook.

Swapping each client's `rpc` + `codec` internals for the Thunder package deletes the
duplicated codecs, closes the pre-allocation DoS gap in every language, and leaves each
SDK's public API, `command_map`, endpoint grammar (`nexus://host:15475`), transport
selection (`NEXUS_SDK_TRANSPORT`) and HTTP fallback untouched.

## What Changes

Per client, the same surgical swap (Synap reference: `sdks/rust/src/transport/mod.rs`):
replace the transport's socket/framing/codec internals with the language's Thunder
client (`connect` with credentials → `call(command, args)` → map typed errors to the
SDK's error type). Value bridging maps the SDK's value type to the Thunder value model
(both already identical in shape: Null/Bool/Int/Float/Bytes/Str/Array/Map).

- CLI: `crates/nexus-cli/src/rpc_transport.rs` wraps `thunder::Client` (client feature
  only); drops direct use of `nexus_protocol::rpc::codec`.
- Rust SDK: `sdks/rust` — `thunder-rpc` `default-features = false, features = ["client"]`
  (registry version, same pin as server so wire ends cannot drift), and **drops its
  `nexus-protocol` dependency entirely** (`sdks/rust/Cargo.toml:32`).
- TypeScript SDK: `@hivehub/thunder` ^0.2.2; replace `src/transports/rpc.ts` + `codec.ts`.
- Python SDK: `hivellm-thunder` >= 0.2.2 (imports `thunder_rpc`; sync client — `aio` is
  available if the SDK later grows async); replace `transport/rpc.py` + `codec.py`.
- Go SDK: `github.com/hivellm/thunder-go` v0.2.2; replace `transport/rpc.go` + `codec.go`.
- C# SDK: `HiveLLM.Thunder` (NuGet, net8.0); replace `Transports/RpcTransport.cs` + `Codec.cs`.
- PHP SDK: `hivellm/thunder`; replace `Transport/RpcTransport.php` + `Codec.php`.

Credentials must be attached to the Thunder client config and sent on every connect
including reconnects (Synap's biggest cross-SDK gotcha: pre-Thunder transports never
sent AUTH; `require_auth` servers broke Go/PHP/C#).

### `nexus-protocol` is deleted here — duplicate, don't share (project decision)

This phase ends with the `crates/nexus-protocol` crate **removed from the workspace**.
Nexus keeps no shared protocol crate after Thunder; anything the server and the Rust
SDK both need is **copied into each side independently**, as Synap did — its SDK
re-declares `synap_protocol_config()` (`sdks/rust/src/transport/mod.rs:58-68`)
byte-identical to the server's `synap_config()` rather than importing it, so the SDK
depends only on registry crates.

What that means concretely:

- The Rust SDK carries its **own** `nexus_thunder_config()` duplicating the server's
  copy from phase10 (scheme, port, handshake, push policy, error convention, frame
  cap), pinned by its own literal-value test. The two tests are the only mechanism
  keeping the wire ends aligned — there is no shared type and no shared crate.
- The phase10 compile shims in `nexus-protocol/src/rpc/` disappear with the crate.
- The crate's non-RPC modules need homes: `rest.rs` (`RestClient`, used by
  `crates/nexus-server/tests/vectorizer_integration_test.rs` and several `examples/`),
  `mcp.rs`, `umicp.rs`, and `resp3/` (used by the server's RESP3 listener) move into
  `nexus-server`, which is unpublished — mirroring Synap, which moved RESP3 and the
  HTTP envelope into `synap-server` precisely so server internals stop being published
  to a registry.
- Do **not** create a replacement `nexus-wire` / `nexus-common` crate. That rebuilds
  what we are deleting.

Two consequences worth flagging:

- **Publishing simplifies.** `sdks/rust/PUBLISH.md:17-31`'s two-step dance (publish
  `nexus-protocol`, wait for the index, then `nexus-graph-sdk`) collapses to publishing
  `nexus-graph-sdk` alone, depending only on registry crates — which is exactly the
  constraint phase13's crates.io lane needs.
- **It permanently supersedes issue #28.** phase15 removes the dead `rmcp` declaration
  from `nexus-protocol` as an immediate unblock for Cortex; deleting the crate removes
  that entire published dependency path for good. phase15 still ships first — it is a
  one-line fix and Cortex is blocked now — but this is the durable end state.

### What does NOT change

- SDK public APIs, `command_map` (dotted method → RPC verb), endpoint grammar and ports,
  transport-selection precedence, the HTTP fallback transport, RESP3 stance (parsed,
  not shipped).
- Each SDK's comprehensive test suite remains the acceptance bar (30+ tests per SDK).

### Risks / notes

- `hivellm/thunder` (PHP) is **not yet on Packagist** — depend via a composer VCS/git
  repository entry until published, or coordinate publication with the Thunder repo
  first. Do not vendor a copy.
- Go/PHP Thunder packages are separate repos (`thunder-go`, `thunder-php`) consumed as
  submodules on the Thunder side; version tags, not branches.
- Server pin (phase10) and SDK pins should be the same thunder version — mirror Synap's
  workspace comment stating the intent.
- Old wire compatibility means SDK migration can ship per-language, in any order, against
  either an old or new server — no lockstep release needed.

## Impact

- Affected specs: `docs/specs/sdk-transport.md`
- Affected code: `crates/nexus-cli/src/{rpc_transport.rs,client.rs}`,
  **deletion of `crates/nexus-protocol/`** with `rest.rs`/`mcp.rs`/`umicp.rs`/`resp3/`
  relocated into `nexus-server`, `sdks/{rust,python,typescript,go,csharp,php}`
  transport layers, per-SDK manifests, `sdks/rust/PUBLISH.md`, workspace `Cargo.toml`
  members, `.github/workflows/sdk-*-test.yml` (if dependency setup changes)
- Breaking change: NO for SDK users — public SDK APIs and the wire are unchanged;
  internal transport classes are replaced. YES for anyone depending on the published
  `nexus-protocol` crate directly, which is discontinued (its types were always
  re-exports of an internal wire; `thunder-rpc` is the supported replacement).
- User benefit: hardened, maintained transports in all languages; duplicated codec code
  deleted (~2.5k LOC across SDKs); SDK bugfixes for framing/reconnect/pipelining now come
  from Thunder upstream instead of six parallel implementations.

## References

- Synap SDK seam: `e:\HiveLLM\Synap\sdks\rust\src\transport\mod.rs` (transport wrap,
  error mapping, copied-not-imported protocol config), `sdks/typescript/package.json`,
  `sdks/python/pyproject.toml`, `sdks/php/composer.json`.
- Thunder client APIs: `e:\HiveLLM\Thunder\{rust/thunder/src/client/,typescript/src/,python/thunder_rpc/,go/client/,csharp/HiveLLM.Thunder/,php/src/Client/}`.
- Nexus current transports: `sdks/*/…/transport*/` (`rpc` + `codec` + `command_map` +
  `endpoint` + factory per language), `crates/nexus-cli/src/rpc_transport.rs`,
  `docs/specs/sdk-transport.md`.
