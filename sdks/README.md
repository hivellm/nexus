# Nexus SDKs

First-party client libraries for the [Nexus graph database](https://github.com/hivellm/nexus).

Every SDK shares the same wire contract documented in
[`docs/specs/sdk-transport.md`](../docs/specs/sdk-transport.md):

- **RPC-first** — the default transport is native binary RPC on port
  `15475` (URL scheme `nexus://`). Measurably 3–10× lower latency
  and 40–60% smaller payloads vs HTTP/JSON.
- **Legacy HTTP fallback** on port `15474` (URL scheme `http://`) for
  restricted networks or when an SDK does not yet have RPC coverage.
- **Canonical tokens**: `TransportMode` string values are single-token
  (`"nexus"`, `"resp3"`, `"http"`) and match the URL scheme. There is
  no `"nexus-rpc"` or `"nexus+rpc"` token.

## Shipping SDKs

| SDK        | Path                              | Package                             | Status    | Tests folder |
|------------|-----------------------------------|-------------------------------------|-----------|--------------|
| Rust       | [`rust/`](rust/)                  | `nexus-sdk` on crates.io            | 1.0.0 — RPC default shipped | `rust/tests/` |
| Python     | [`python/`](python/)              | `nexus-sdk` on PyPI                 | 1.0.0 — RPC default queued  | `python/tests/` |
| TypeScript | [`typescript/`](typescript/)      | `@hivellm/nexus-sdk` on npm         | 1.0.0 — RPC default queued  | `typescript/tests/` |
| Go         | [`go/`](go/)                      | `github.com/hivellm/nexus-go`       | 1.0.0 — RPC default queued  | `go/*_test.go` |
| C#         | [`csharp/`](csharp/)              | `Nexus.SDK` on nuget.org            | 1.0.0 — RPC default queued  | `csharp/Tests/` |
| PHP        | [`php/`](php/)                    | `hivellm/nexus-php` on packagist    | 1.0.0 — RPC default queued  | `php/tests/` |

RPC transport roll-out is tracked by
[`phase2_sdk-rpc-transport-default`](../.rulebook/tasks/phase2_sdk-rpc-transport-default/);
the Rust SDK is the reference implementation (sections 1 + 2 of the
task). Other SDKs follow the same shape — endpoint parser + Transport
trait + RPC impl + HTTP fallback + command map — using the Rust SDK
and the spec as the source of truth.

## Quick start

Each SDK's own README has per-language install and usage. The shape
is consistent:

```
Rust:       NexusClient::new("nexus://127.0.0.1:15475")
Python:     NexusClient(base_url="nexus://127.0.0.1:15475")
TypeScript: new NexusClient({ baseUrl: "nexus://127.0.0.1:15475" })
Go:         nexus.NewClient("nexus://127.0.0.1:15475")
C#:         new NexusClient("nexus://127.0.0.1:15475")
PHP:        new NexusClient("nexus://127.0.0.1:15475")
```

Override per process via `NEXUS_SDK_TRANSPORT` (`nexus` / `http` /
`https` / `resp3` / `auto`); override per client via the config
object's `transport` field.

## Cross-SDK test matrix

```powershell
pwsh sdks/run-all-comprehensive-tests.ps1
```

The script runs every SDK's comprehensive suite in sequence and
prints a summary. Each SDK also has its own native test command —
see the SDK READMEs.

## Historical test reports

Past run reports live under
[`docs/sdks/`](../docs/sdks/):

- [`SDK_TEST_RESULTS.md`](../docs/sdks/SDK_TEST_RESULTS.md) —
  first comprehensive pass (2025-12).
- [`SDK_TEST_RESULTS_FINAL.md`](../docs/sdks/SDK_TEST_RESULTS_FINAL.md) —
  final pre-1.0.0 HTTP-only pass.
- [`TEST_COVERAGE_REPORT.md`](../docs/sdks/TEST_COVERAGE_REPORT.md) —
  coverage snapshot per SDK.

These predate the 1.0.0 cut and reference SDKs that were since
dropped (n8n, langchain, langflow, TestConsoleSimple). They are kept
for audit / regression comparisons; new test runs should live under
each SDK's own test directory.

## Removed SDKs

The following ecosystem integrations were removed in the 1.0.0 cut to
focus on first-party wire clients:

- **n8n** — community node (`sdks/n8n/`). Users needing n8n can
  invoke the Nexus HTTP endpoint directly or build a thin wrapper
  over the TypeScript SDK.
- **LangChain** (`sdks/langchain/`) and **LangFlow**
  (`sdks/langflow/`) — Python ecosystem wrappers. The underlying
  Python SDK covers the same API surface; higher-level orchestration
  wrappers are better maintained out-of-tree where they can track
  upstream LangChain / LangFlow releases on their own cadence.
- **TestConsoleSimple** — redundant C# test harness; the canonical
  C# tests live in `sdks/csharp/Tests/`.

## License

See each SDK directory for its own license file. All first-party
SDKs are licensed under Apache-2.0 unless otherwise noted in the
per-SDK metadata.
