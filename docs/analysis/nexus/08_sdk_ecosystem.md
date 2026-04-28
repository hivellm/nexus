# 08 — SDK & Ecosystem

## Per-SDK matrix

All 6 first-party SDKs lockstep at **v1.15.0** (2026-04 release train). Server core is at v1.13.0 / branch `release/v1.2.0`; `nexus-protocol` crate at v1.14.0. Versions diverge by intent — SDKs ride a faster release cadence than the engine.

| SDK | Version | Package | Transports | Test count | Status |
|-----|---------|---------|------------|-----------|--------|
| **Rust** | 1.15.0 | `nexus-sdk` (crates.io) | RPC default, HTTP, RESP3 | 30+ comprehensive | ✅ stable |
| **TypeScript / JS** | 1.15.0 | **`@hivehub/nexus-sdk`** (npm — note scope drift; previously `@hivellm/`) | RPC default, HTTP, RESP3 | 30+ comprehensive | ✅ stable, ESM + CJS |
| **Python** | 1.15.0 | `nexus-sdk` (PyPI) — minimum Python 3.8 | RPC, HTTP | 30+ comprehensive | ✅ stable |
| **Go** | 1.15.0 | `github.com/hivellm/nexus-go` — Go 1.21+ | RPC, HTTP | 30+ comprehensive | ✅ stable, includes `RowsAsMap()` helper |
| **C# / .NET** | 1.15.0 | `Nexus.SDK` (NuGet) | RPC, HTTP | 30+ via `dotnet test` | ✅ stable, includes `RowsAsMap()` helper |
| **PHP** | 1.15.0 | `hivellm/nexus-sdk` (Composer) | HTTP (no RPC yet — verify) | 30+ via PHPUnit | ✅ stable |

**Memory note (from project memory):** TypeScript SDK npm scope is **`@hivehub`**, not `@hivellm`. GitHub repo URL unchanged. Other SDKs unchanged.

### Row-format compatibility patterns

Server returns Neo4j-array format `[[v1, v2], ...]` always. Languages split into two camps:

- **Flexible-typed** (Python `list[Any]`, TypeScript `any[]`, Rust `serde_json::Value`, PHP native arrays) — work directly with the array shape.
- **Strict-typed** (Go `[][]interface{}`, C# `List<List<object?>>`) — ship a `RowsAsMap()` helper to flatten into named records.

Server format is **never** changed to suit a single SDK — that's the rule that protects the 300/300 Neo4j compatibility.

## CLI

`nexus-cli` (binary `nexus`) defaults to `nexus://127.0.0.1:15475` (RPC). Subcommands:

| Subcommand | Purpose |
|------------|---------|
| `query` | Run Cypher, render table |
| `db` | List / create / drop / switch databases |
| `user` | Manage users + roles |
| `key` | Manage API keys |
| `schema` | Inspect labels / types / properties / constraints / indexes |
| `data` | Bulk import / export |

All subcommands flow through RPC. CLI README is at `crates/nexus-cli/README.md`. CLI ships in the same release train as the server (single binary install via `scripts/install/install.{sh,ps1}`).

## Transport stack

| Transport | Port (default) | Format | Use |
|-----------|---------------|--------|-----|
| **Binary RPC** | 15475 | Length-prefixed MessagePack | Default for CLI + all SDKs; 3–10× lower latency vs HTTP/JSON |
| **HTTP/JSON** | 15474 | REST `POST /cypher`, `POST /knn_traverse`, etc. | Web compatibility, debugging, browser fetch |
| **RESP3** | configurable | Redis-style RESP3 | FalkorDB-adjacent clients |

**Auth across transports** — API keys + JWT supported on all three. mTLS configurable (V2). Rate limits per-API-key apply uniformly.

**KNN bytes-native** — embeddings sent as `bytes` on RPC wire (no base64 round-trip). HTTP path still requires base64 — measurable hit at d=768.

## Authentication support per SDK

| Auth type | Rust | TS | Python | Go | C# | PHP |
|-----------|------|----|---------|-----|-----|-----|
| API key | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| JWT | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| mTLS client cert | partial — depends on each language's TLS stack; needs verification per-SDK | | | | | |

## Documentation status

- **Each SDK ships:** `README.md` (quickstart + API ref), `CHANGELOG.md`, `LICENSE` (Apache 2.0), comprehensive test file.
- **TypeScript** ships ESM + CJS dual exports + types, plus `examples/` directory.
- **Rust** ships `PUBLISH.md` (release process), pinned `nexus-protocol` path-and-version dual dep (required by crates.io).
- **Python** declares `python_version = "3.8"` minimum, supports 3.8 / 3.9 / 3.10 / 3.11 / 3.12.
- **No "GraphRAG quickstart" cookbook** in any SDK — gap.
- **No LangChain / LlamaIndex / Haystack integrations** packaged — gap.

## Distribution channels

| Channel | Status |
|---------|--------|
| **crates.io** | `nexus-sdk` (Rust), `nexus-protocol`, `nexus-core`, `nexus-cli` published per release |
| **npm** | `@hivehub/nexus-sdk` v1.15.0 |
| **PyPI** | `nexus-sdk` v1.15.0 |
| **NuGet** | `Nexus.SDK` v1.15.0 |
| **Packagist (Composer)** | `hivellm/nexus-sdk` v1.15.0 |
| **Go modules** | `github.com/hivellm/nexus-go` v1.15.0 |
| **Maven Central / JVM** | **NOT PUBLISHED — JVM SDK does not exist** |
| **Docker Hub / GHCR** | unknown — requires audit of CI artifacts |
| **Helm chart repo** | not published |

## Gaps vs the Neo4j driver ecosystem

| Driver / library | Neo4j ships | Nexus ships |
|------------------|-------------|-------------|
| Java / Kotlin (JVM) | ✅ canonical | ❌ |
| Spring Data Neo4j | ✅ | ❌ |
| .NET full driver | ✅ | ✅ (Nexus.SDK) |
| Python | ✅ | ✅ |
| JavaScript / TS | ✅ | ✅ |
| Go | ✅ (community) | ✅ |
| Rust | ❌ (community-only at Neo4j) | ✅ first-party |
| PHP | community | ✅ first-party |
| Bolt protocol compatibility | n/a (Neo4j *is* Bolt) | ❌ — Nexus uses bespoke RPC |
| Cypher LSP | community | ❌ |
| IDE plugins (VS Code / IntelliJ) | community | ❌ |

The **Rust + PHP first-party SDKs are a Nexus advantage** Neo4j doesn't match. The **JVM gap is the biggest friction point** for enterprise sales — every existing Neo4j Java app would need a rewrite.

## Recommended additions (priority order)

1. **JVM SDK (Java + Kotlin coroutines)** — ~4 weeks. Single biggest enterprise-adoption gate.
2. **Bolt-protocol shim** on a separate port — ~3–4 weeks. Lets existing Neo4j drivers connect directly. Highest leverage for "drop-in Neo4j replacement" narrative.
3. **GraphRAG / LangChain / LlamaIndex integration pack** — published as a Python sub-package + a TypeScript adapter. ~2–3 weeks.
4. **Cypher LSP** — VS Code + IntelliJ extensions. ~1–2 weeks if forked from existing Cypher LSPs.
5. **Helm chart** + K8s operator + Docker Compose example pack — ~2 weeks. Cloud-native gate.
6. **Spring Data Nexus** — depends on JVM SDK landing. ~2–3 weeks after.
7. **WASM build** of `nexus-core` + JS bindings — capture the Kuzu-WASM vacancy. ~4–6 weeks.
8. **Migration guides FROM Neo4j / Kuzu / Memgraph** — 1 week each. Low effort, high narrative impact.
9. **Reconcile npm scope** to a single canonical (`@hivehub` per memory) and document the rename.
10. **Verify mTLS path** in each SDK and document supported versions of the underlying TLS stack.
